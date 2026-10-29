//! Opt-in second-monitor panels on one private IPv4 interface. Trusted-LAN HTTP,
//! not Internet remote operation. A short local code exchanges for a tab-memory token for panel reads
//! and ONE receive-only operation. No arbitrary command dispatcher is exposed.
//! Stopping sharing revokes every link and in-flight request; restart is always off.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::Manager;
use tempo_app::engine::engine_try_lock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::lan_client::origin::{parse_head, Head};
use crate::{SharedEngine, TV_APP};

const PORT: u16 = 7375;
const MAX_HEAD: usize = 8192;
const MAX_BODY: usize = 512;
const DEADLINE: Duration = Duration::from_secs(8);
const READS: &[&str] = &["state", "get_ota_spots", "get_need_alerts", "get_band_plan"];
type ReadCell = Arc<Mutex<Option<(Instant, Value)>>>;
type ReceiveSelection = Arc<dyn Fn(&Listen) -> Result<(), String> + Send + Sync>;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Action {
    Status,
    Start {
        address: Option<String>,
        port: Option<u16>,
    },
    Stop,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    running: bool,
    address: String,
    port: u16,
    /// Explicitly requested at the local screen only; never logged or persisted.
    links: HashMap<&'static str, String>,
    access_code: Option<String>,
}

struct Session {
    base: String,
    address: Ipv4Addr,
    port: u16,
    access: String,
    code: String,
    code_tag: hmac::Tag,
    pair_attempts: Mutex<(Instant, u8)>,
    auth_key: hmac::Key,
    auth_tag: hmac::Tag,
    active: AtomicBool,
    stop: tokio::sync::watch::Sender<bool>,
    last_click: Mutex<Instant>,
    cache: Mutex<HashMap<String, ReadCell>>,
    work: Arc<tokio::sync::Semaphore>,
    tune: ReceiveSelection,
}

static SESSION: OnceLock<tokio::sync::Mutex<Option<Arc<Session>>>> = OnceLock::new();

fn hub() -> &'static tokio::sync::Mutex<Option<Arc<Session>>> {
    SESSION.get_or_init(|| tokio::sync::Mutex::new(None))
}

fn status(session: Option<&Session>) -> Status {
    let mut links = HashMap::new();
    if let Some(s) = session {
        for panel in ["pota", "conditions", "needed"] {
            links.insert(panel, s.base.clone());
        }
    }
    Status {
        running: session.is_some(),
        address: session
            .map(|s| s.address.to_string())
            .or_else(crate::lan_ip_hint)
            .unwrap_or_default(),
        port: session.map_or(PORT, |s| s.port),
        links,
        access_code: session.map(|s| s.code.clone()),
    }
}

pub(crate) async fn action(
    origin: crate::remote_service::Origin,
    action: Action,
) -> Result<Status, String> {
    if origin != crate::remote_service::Origin::Shack {
        return Err("Browser sharing can only be changed at the laptop".into());
    }
    let mut slot = hub().lock().await;
    if slot
        .as_ref()
        .is_some_and(|s| !s.active.load(Ordering::SeqCst))
    {
        *slot = None;
    }
    match action {
        Action::Status => {}
        Action::Stop => {
            if let Some(s) = slot.take() {
                let _click = s.last_click.lock().map_err(|_| "Sharing unavailable")?;
                s.active.store(false, Ordering::SeqCst);
                let _ = s.stop.send(true);
            }
        }
        Action::Start { address, port } => {
            if slot.is_none() {
                let ip: Ipv4Addr = address
                    .or_else(crate::lan_ip_hint)
                    .ok_or("No LAN address found; enter the laptop's private IPv4 address")?
                    .parse()
                    .map_err(|_| "Enter a private IPv4 address")?;
                if !tempo_stream::lan::listenable(ip) {
                    return Err("Sharing must use a private LAN address, not all interfaces or a public address".into());
                }
                let port = port.unwrap_or(PORT);
                if port < 1024 {
                    return Err("Use a port from 1024 to 65535".into());
                }
                let listener = TcpListener::bind((ip, port))
                    .await
                    .map_err(|e| format!("Cannot share on that address: {e}"))?;
                let mut secret = [0u8; 32];
                SystemRandom::new()
                    .fill(&mut secret)
                    .map_err(|_| "Cannot make an access link")?;
                let access: String = secret.iter().map(|b| format!("{b:02x}")).collect();
                let auth_key = hmac::Key::new(hmac::HMAC_SHA256, &secret);
                let auth_tag = hmac::sign(&auth_key, access.as_bytes());
                let code = new_code()?;
                let code_tag = hmac::sign(&auth_key, code.as_bytes());
                let (stop, mut stopped) = tokio::sync::watch::channel(false);
                let s = Arc::new(Session {
                    base: format!("http://{ip}:{port}"),
                    address: ip,
                    port,
                    access,
                    code,
                    code_tag,
                    pair_attempts: Mutex::new((Instant::now(), 0)),
                    auth_key,
                    auth_tag,
                    active: AtomicBool::new(true),
                    stop,
                    last_click: Mutex::new(Instant::now() - Duration::from_secs(1)),
                    cache: Mutex::new(HashMap::new()),
                    work: Arc::new(tokio::sync::Semaphore::new(16)),
                    tune: Arc::new(apply_listen),
                });
                let running = s.clone();
                tauri::async_runtime::spawn(async move {
                    let permits = Arc::new(tokio::sync::Semaphore::new(24));
                    loop {
                        tokio::select! {
                            _ = stopped.changed() => break,
                            accepted = listener.accept() => {
                                let Ok((stream, peer)) = accepted else { break };
                                if !peer.ip().is_ipv4() || !tempo_stream::lan::private(peer.ip()) { continue; }
                                let Ok(permit) = permits.clone().try_acquire_owned() else { continue };
                                let session = running.clone();
                                tauri::async_runtime::spawn(async move {
                                    let _permit = permit;
                                    let _ = tokio::time::timeout(DEADLINE, serve(stream, session)).await;
                                });
                            }
                        }
                    }
                    running.active.store(false, Ordering::SeqCst);
                });
                *slot = Some(s);
            }
        }
    }
    Ok(status(
        slot.as_deref().filter(|s| s.active.load(Ordering::SeqCst)),
    ))
}

fn new_code() -> Result<String, String> {
    // Rejection sampling keeps all eight-digit codes equally likely.
    loop {
        let mut bytes = [0u8; 4];
        SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| "Cannot make an access code")?;
        let value = u32::from_le_bytes(bytes);
        if value < 4_200_000_000 {
            return Ok(format!("{:08}", value % 100_000_000));
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pair {
    code: String,
}

fn pair(s: &Session, request: Pair) -> Result<Value, String> {
    let mut attempts = s.pair_attempts.lock().map_err(|_| "Sharing unavailable")?;
    if !s.active.load(Ordering::SeqCst) {
        return Err("Sharing stopped".into());
    }
    if attempts.0.elapsed() >= Duration::from_secs(30) {
        *attempts = (Instant::now(), 0);
    }
    if attempts.1 >= 5 {
        return Err("Too many attempts; wait 30 seconds and try again".into());
    }
    attempts.1 += 1;
    if request.code.len() != 8
        || !request.code.bytes().all(|b| b.is_ascii_digit())
        || hmac::verify(&s.auth_key, request.code.as_bytes(), s.code_tag.as_ref()).is_err()
    {
        return Err("Incorrect access code; check Nexus on the laptop".into());
    }
    Ok(json!({ "access": s.access }))
}

fn one<'a>(h: &'a Head, key: &str) -> Option<&'a str> {
    let mut values = h
        .headers
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.as_str());
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

fn admitted(s: &Session, head: &Head) -> bool {
    s.active.load(Ordering::SeqCst)
        && one(head, "host") == s.base.strip_prefix("http://")
        && head
            .headers
            .iter()
            .filter(|(key, _)| key == "origin")
            .count()
            <= 1
        && one(head, "origin").is_none_or(|origin| origin == s.base)
        && one(head, "authorization")
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|token| {
                hmac::verify(&s.auth_key, token.as_bytes(), s.auth_tag.as_ref()).is_ok()
            })
}

fn read_allowed(cmd: &str) -> bool {
    READS.contains(&cmd) || tempo_app::connect_web::RPC_ALLOWLIST.contains(&cmd)
}

async fn serve(mut stream: TcpStream, s: Arc<Session>) -> Result<(), String> {
    let mut bytes = Vec::new();
    let end = loop {
        if let Some(n) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break n + 4;
        }
        if bytes.len() >= MAX_HEAD {
            return Err("Request too large".into());
        }
        let mut buf = [0; 1024];
        let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(());
        }
        bytes.extend_from_slice(&buf[..n]);
    };
    if end > MAX_HEAD {
        return Err("Request too large".into());
    }
    let head = parse_head(&bytes[..end - 4]).ok_or("Invalid request")?;
    let url =
        tauri::Url::parse(&format!("{}{}", s.base, head.target)).map_err(|_| "Invalid path")?;
    let path = url.path();
    let reply: Result<(String, Vec<u8>), String> = if head.method == "GET"
        && (path == "/" || ["/panels/pota", "/panels/conditions", "/panels/needed"].contains(&path))
    {
        crate::tv_asset("observer.html")
            .map(|(html, mime)| {
                (
                    mime,
                    String::from_utf8_lossy(&html)
                        .replace("./assets/", "/assets/")
                        .into_bytes(),
                )
            })
            .ok_or("Browser panel assets are unavailable; rebuild the UI".into())
    } else if head.method == "GET" && path.starts_with("/assets/") && !path.contains("..") {
        crate::tv_asset(path.trim_start_matches('/'))
            .map(|(body, mime)| (mime, body))
            .ok_or("Asset not found".into())
    } else if head.method == "POST" && path == "/observer/pair" {
        if !s.active.load(Ordering::SeqCst)
            || one(&head, "host") != s.base.strip_prefix("http://")
            || one(&head, "origin") != Some(s.base.as_str())
            || one(&head, "content-type") != Some("application/json")
            || head.headers.iter().any(|(k, _)| k == "transfer-encoding")
        {
            return send(&mut stream, 403, "text/plain", b"Access request refused").await;
        }
        let Some(length) = one(&head, "content-length")
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|&n| n > 0 && n <= MAX_BODY)
        else {
            return send(&mut stream, 400, "text/plain", b"Invalid body length").await;
        };
        while bytes.len() < end + length {
            let mut buf = [0; MAX_BODY];
            let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("Incomplete body".into());
            }
            bytes.extend_from_slice(&buf[..n]);
        }
        let Ok(request) = serde_json::from_slice::<Pair>(&bytes[end..end + length]) else {
            return send(&mut stream, 400, "text/plain", b"Invalid access request").await;
        };
        match pair(&s, request) {
            Ok(value) => Ok((
                "application/json".into(),
                serde_json::to_vec(&value).map_err(|e| e.to_string())?,
            )),
            Err(why) => return send(&mut stream, 403, "text/plain", why.as_bytes()).await,
        }
    } else if !admitted(&s, &head) {
        return send(
            &mut stream,
            403,
            "text/plain",
            b"Enter the current access code shown in Nexus",
        )
        .await;
    } else if head.method == "GET" && path.starts_with("/observer/read/") {
        let cmd = path.trim_start_matches("/observer/read/").to_string();
        if !read_allowed(&cmd) {
            return send(&mut stream, 403, "text/plain", b"Operation not allowed").await;
        }
        let args = url
            .query_pairs()
            .find(|(k, _)| k == "args")
            .map(|(_, v)| v.into_owned())
            .unwrap_or_else(|| "{}".into());
        if args.len() > 2048 {
            return send(&mut stream, 400, "text/plain", b"Arguments too large").await;
        }
        let session = s.clone();
        let Ok(work) = s.work.clone().try_acquire_owned() else {
            return send(
                &mut stream,
                409,
                "text/plain",
                b"Panels are busy; try again",
            )
            .await;
        };
        tauri::async_runtime::spawn_blocking(move || {
            let _work = work;
            cached_read(&session, &cmd, &args)
        })
        .await
        .map_err(|_| "Panel read failed")?
        .and_then(|v| {
            serde_json::to_vec(&v)
                .map(|b| ("application/json".into(), b))
                .map_err(|e| e.to_string())
        })
    } else if head.method == "POST" && path == "/observer/listen" {
        if one(&head, "origin") != Some(s.base.as_str())
            || one(&head, "content-type") != Some("application/json")
            || head.headers.iter().any(|(k, _)| k == "transfer-encoding")
        {
            return send(&mut stream, 403, "text/plain", b"Invalid tuning request").await;
        }
        let Some(length) = one(&head, "content-length")
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|&n| n > 0 && n <= MAX_BODY)
        else {
            return send(&mut stream, 400, "text/plain", b"Invalid body length").await;
        };
        while bytes.len() < end + length {
            let mut buf = [0; MAX_BODY];
            let n = stream.read(&mut buf).await.map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("Incomplete body".into());
            }
            bytes.extend_from_slice(&buf[..n]);
        }
        let Ok(request) = serde_json::from_slice::<Listen>(&bytes[end..end + length]) else {
            return send(&mut stream, 400, "text/plain", b"Invalid tuning request").await;
        };
        let session = s.clone();
        let Ok(work) = s.work.clone().try_acquire_owned() else {
            return send(
                &mut stream,
                409,
                "text/plain",
                b"Panels are busy; try again",
            )
            .await;
        };
        tauri::async_runtime::spawn_blocking(move || {
            let _work = work;
            listen(&session, request)
        })
        .await
        .map_err(|_| "Tuning failed")?
        .map(|_| ("application/json".into(), b"{\"accepted\":true}".to_vec()))
    } else {
        return send(&mut stream, 403, "text/plain", b"Operation not allowed").await;
    };
    match reply {
        Ok((mime, bytes)) => send(&mut stream, 200, &mime, &bytes).await,
        Err(why) => send(&mut stream, 409, "text/plain", why.as_bytes()).await,
    }
}

async fn send(stream: &mut TcpStream, code: u16, mime: &str, body: &[u8]) -> Result<(), String> {
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        _ => "Conflict",
    };
    let head = format!("HTTP/1.1 {code} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nX-Frame-Options: DENY\r\nContent-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https:; font-src 'self' data:; connect-src 'self'; worker-src 'self' blob:; frame-ancestors 'none'; base-uri 'none'; object-src 'none'\r\n\r\n", body.len());
    stream
        .write_all(head.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    stream.write_all(body).await.map_err(|e| e.to_string())
}

fn now_ms() -> u64 {
    tempo_core::timing::now_unix_ms() as u64
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Listen {
    radio: u32,
    mhz: f64,
    mode: String,
    observed_at: u64,
}

fn listen(s: &Session, request: Listen) -> Result<(), String> {
    let now = now_ms();
    if !s.active.load(Ordering::SeqCst)
        || request.observed_at > now
        || now.saturating_sub(request.observed_at) > 3000
    {
        return Err("The panel state expired; refresh and click again".into());
    }
    let mut last = s
        .last_click
        .try_lock()
        .map_err(|_| "Another selection is in progress; click again")?;
    if last.elapsed() < Duration::from_millis(300) {
        return Err("Wait a moment before tuning again".into());
    }
    // Recheck under the same short-lived locks as Stop sharing and radio selection.
    if !s.active.load(Ordering::SeqCst) {
        return Err("Sharing stopped".into());
    }
    (s.tune)(&request)?;
    *last = Instant::now();
    Ok(())
}

fn apply_listen(request: &Listen) -> Result<(), String> {
    let app = TV_APP.get().ok_or("Still starting up")?;
    let engine = app.state::<SharedEngine>();
    let mut e = engine_try_lock(&engine).map_err(|_| "The laptop is busy; click again")?;
    e.observer_listen(request.radio, request.mhz, &request.mode)
}

fn cached_read(s: &Session, cmd: &str, args: &str) -> Result<Value, String> {
    if !s.active.load(Ordering::SeqCst) || !read_allowed(cmd) {
        return Err("Operation not allowed".into());
    }
    if cmd == "state" {
        let app = TV_APP.get().ok_or("Still starting up")?;
        let engine = app.state::<SharedEngine>();
        let e = engine_try_lock(&engine).map_err(|_| "The laptop is busy")?;
        let radio = e.snapshot().radio;
        return Ok(
            json!({ "radio": e.settings().active_radio, "mhz": e.settings().dial_mhz,
            "band": e.settings().band, "mode": e.settings().operating_mode, "tier": e.tier(),
            "catOk": radio.cat_ok, "busy": radio.rig_keyed || e.tx_owner().is_some(), "observedAt": now_ms() }),
        );
    }
    let key = format!("{cmd}:{args}");
    let cell = {
        let mut cache = s.cache.lock().map_err(|_| "Panel cache unavailable")?;
        if cache.len() >= 64 && !cache.contains_key(&key) {
            return Err("Too many panel requests".into());
        }
        cache.entry(key).or_default().clone()
    };
    let mut entry = cell.lock().map_err(|_| "Panel cache unavailable")?;
    if let Some((at, value)) = &*entry {
        if at.elapsed() < Duration::from_secs(30) {
            return Ok(value.clone());
        }
    }
    let result = crate::observer_read(cmd, args)?;
    if !s.active.load(Ordering::SeqCst) {
        return Err("Sharing stopped".into());
    }
    *entry = Some((Instant::now(), result.clone()));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session() -> Arc<Session> {
        let key = hmac::Key::new(hmac::HMAC_SHA256, b"test-only-access");
        let tag = hmac::sign(&key, b"test-only-access");
        let (stop, _) = tokio::sync::watch::channel(false);
        Arc::new(Session {
            base: "http://192.168.1.10:7375".into(),
            address: Ipv4Addr::new(192, 168, 1, 10),
            port: PORT,
            access: "test-only-access".into(),
            code: "12345678".into(),
            code_tag: hmac::sign(&key, b"12345678"),
            pair_attempts: Mutex::new((Instant::now(), 0)),
            auth_key: key,
            auth_tag: tag,
            active: AtomicBool::new(true),
            stop,
            last_click: Mutex::new(Instant::now() - Duration::from_secs(1)),
            cache: Mutex::new(HashMap::new()),
            work: Arc::new(tokio::sync::Semaphore::new(16)),
            tune: Arc::new(apply_listen),
        })
    }

    async fn talk(s: Arc<Session>, request: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _ = serve(stream, s).await;
        });
        let mut socket = TcpStream::connect(addr).await.unwrap();
        socket.write_all(request.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(3), socket.read_to_end(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        task.await.unwrap();
        String::from_utf8(bytes).unwrap()
    }

    #[tokio::test]
    async fn observer_short_code_exchange_requires_origin_and_never_puts_access_in_address() {
        let s = test_session();
        let body = r#"{"code":"12345678"}"#;
        let request = format!("POST /observer/pair HTTP/1.1\r\nHost: 192.168.1.10:7375\r\nOrigin: http://192.168.1.10:7375\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        let response = talk(s.clone(), &request).await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains(r#""access":"test-only-access""#));
        for link in status(Some(&s)).links.values() {
            assert_eq!(link, "http://192.168.1.10:7375");
        }
        assert!(talk(s.clone(), &request.replace("12345678", "87654321"))
            .await
            .starts_with("HTTP/1.1 403"));
        assert!(talk(
            s.clone(),
            &request.replace(
                "Origin: http://192.168.1.10:7375",
                "Origin: http://attacker.invalid"
            )
        )
        .await
        .starts_with("HTTP/1.1 403"));
        s.active.store(false, Ordering::SeqCst);
        assert!(talk(s, &request).await.starts_with("HTTP/1.1 403"));
    }

    #[test]
    fn observer_code_guesses_are_bounded_and_reset_after_the_window() {
        let s = test_session();
        for _ in 0..5 {
            assert!(pair(
                &s,
                Pair {
                    code: "87654321".into()
                }
            )
            .unwrap_err()
            .contains("Incorrect"));
        }
        assert!(pair(
            &s,
            Pair {
                code: "12345678".into()
            }
        )
        .unwrap_err()
        .contains("Too many"));
        *s.pair_attempts.lock().unwrap() = (Instant::now() - Duration::from_secs(31), 5);
        assert_eq!(
            pair(
                &s,
                Pair {
                    code: "12345678".into()
                }
            )
            .unwrap()["access"],
            "test-only-access"
        );
        assert!(serde_json::from_value::<Pair>(json!({"code":"12345678", "arm":true})).is_err());
        for _ in 0..10 {
            let code = new_code().unwrap();
            assert_eq!(code.len(), 8);
            assert!(code.bytes().all(|b| b.is_ascii_digit()));
        }
    }

    #[tokio::test]
    async fn observer_real_socket_refuses_transmit_generic_bridge_csrf_and_extra_arming_fields() {
        let s = test_session();
        for path in [
            "set_tune",
            "set_tx_enabled",
            "work_spot",
            "call_station",
            "get_settings",
            "observer_panel_action",
        ] {
            let request = format!("GET /observer/read/{path} HTTP/1.1\r\nHost: 192.168.1.10:7375\r\nAuthorization: Bearer test-only-access\r\n\r\n");
            assert!(
                talk(s.clone(), &request).await.starts_with("HTTP/1.1 403"),
                "{path}"
            );
        }
        let unauthorized = "GET /observer/read/state HTTP/1.1\r\nHost: 192.168.1.10:7375\r\n\r\n";
        assert!(talk(s.clone(), unauthorized)
            .await
            .starts_with("HTTP/1.1 403"));
        let body = r#"{"radio":0,"mhz":14.074,"mode":"FT8","observedAt":0,"arm":true}"#;
        let post = format!("POST /observer/listen HTTP/1.1\r\nHost: 192.168.1.10:7375\r\nOrigin: http://192.168.1.10:7375\r\nAuthorization: Bearer test-only-access\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        assert!(talk(s.clone(), &post).await.starts_with("HTTP/1.1 400"));
        assert!(talk(
            s.clone(),
            &post.replace(
                "Origin: http://192.168.1.10:7375",
                "Origin: http://attacker.invalid"
            )
        )
        .await
        .starts_with("HTTP/1.1 403"));
        // Positive control: a valid read reaches the dispatcher (no app in this unit test).
        let allowed = "GET /observer/read/state HTTP/1.1\r\nHost: 192.168.1.10:7375\r\nAuthorization: Bearer test-only-access\r\n\r\n";
        let result = talk(s.clone(), allowed).await;
        assert!(result.starts_with("HTTP/1.1 409") && result.contains("Still starting up"));
        s.active.store(false, Ordering::SeqCst);
        assert!(talk(s, allowed).await.starts_with("HTTP/1.1 403"));
    }

    #[test]
    fn observer_click_expiry_and_revocation_are_checked_before_any_engine_access() {
        let s = test_session();
        let expired = Listen {
            radio: 0,
            mhz: 14.074,
            mode: "FT8".into(),
            observed_at: now_ms().saturating_sub(4000),
        };
        assert!(listen(&s, expired).unwrap_err().contains("expired"));
        s.active.store(false, Ordering::SeqCst);
        let revoked = Listen {
            radio: 0,
            mhz: 14.074,
            mode: "FT8".into(),
            observed_at: now_ms(),
        };
        assert!(listen(&s, revoked).is_err());
    }

    #[tokio::test]
    async fn observer_authorized_socket_reaches_receive_selection_but_leaves_a_real_engine_disarmed(
    ) {
        let engine = Arc::new(Mutex::new(tempo_app::engine::Engine::new(
            "W9XYZ", "EN37", 0,
        )));
        let radio = {
            let mut e = engine.lock().unwrap();
            e.set_license_class("extra");
            e.set_cat_status(Some(true), String::new());
            e.set_tier(tempo_app::dto::Tier::Ft8);
            e.set_frequency(14.074, "20m", "USB");
            e.call_cq(None).unwrap();
            assert!(e.tx_enabled());
            e.settings().active_radio
        };
        let mut session = test_session();
        let target = engine.clone();
        Arc::get_mut(&mut session).unwrap().tune = Arc::new(move |request| {
            target
                .try_lock()
                .map_err(|_| "Busy".to_string())?
                .observer_listen(request.radio, request.mhz, &request.mode)
        });
        let body =
            json!({"radio":radio,"mhz":14.08,"mode":"FT4","observedAt":now_ms()}).to_string();
        let request = format!("POST /observer/listen HTTP/1.1\r\nHost: 192.168.1.10:7375\r\nOrigin: http://192.168.1.10:7375\r\nAuthorization: Bearer test-only-access\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len());
        assert!(talk(session, &request).await.starts_with("HTTP/1.1 200"));
        let mut e = engine.lock().unwrap();
        assert_eq!(e.settings().dial_mhz, 14.08);
        assert_eq!(e.tier(), tempo_app::dto::Tier::Ft4);
        assert!(!e.tx_enabled());
        assert!(e.tx_owner().is_none());
        for slot in 1..5 {
            assert!(e.poll_tx(slot).is_empty());
        }
    }

    #[tokio::test]
    async fn observer_sharing_can_only_be_enabled_locally_and_only_on_private_addresses() {
        assert!(
            action(crate::remote_service::Origin::Stream, Action::Status)
                .await
                .is_err()
        );
        for address in ["0.0.0.0", "8.8.8.8", "127.0.0.1"] {
            assert!(action(
                crate::remote_service::Origin::Shack,
                Action::Start {
                    address: Some(address.into()),
                    port: Some(PORT)
                }
            )
            .await
            .is_err());
        }
    }

    #[test]
    fn observer_allowlist_excludes_every_transmit_and_generic_command() {
        assert!(read_allowed("get_need_alerts"));
        assert!(read_allowed("get_propagation"));
        for command in [
            "set_tune",
            "atu_tune",
            "set_tx_enabled",
            "set_mode",
            "work_spot",
            "call_station",
            "set_settings",
            "get_settings",
            "log_current_qso",
            "get_snapshot",
        ] {
            assert!(!read_allowed(command), "{command}");
        }
        assert!(serde_json::from_value::<Listen>(
            json!({"radio":0,"mhz":14.074,"mode":"FT8","observedAt":0,"arm":true})
        )
        .is_err());
    }

    #[test]
    fn observer_admission_requires_current_link_host_and_origin() {
        let key = hmac::Key::new(hmac::HMAC_SHA256, b"test-only-access");
        let tag = hmac::sign(&key, b"test-only-access");
        let (stop, _) = tokio::sync::watch::channel(false);
        let s = Session {
            base: "http://192.168.1.10:7375".into(),
            address: Ipv4Addr::new(192, 168, 1, 10),
            port: PORT,
            access: "test-only-access".into(),
            code: "12345678".into(),
            code_tag: hmac::sign(&key, b"12345678"),
            pair_attempts: Mutex::new((Instant::now(), 0)),
            auth_key: key,
            auth_tag: tag,
            active: AtomicBool::new(true),
            stop,
            last_click: Mutex::new(Instant::now()),
            cache: Mutex::new(HashMap::new()),
            work: Arc::new(tokio::sync::Semaphore::new(16)),
            tune: Arc::new(apply_listen),
        };
        let valid = "GET /observer/read/state HTTP/1.1\r\nHost: 192.168.1.10:7375\r\nAuthorization: Bearer test-only-access";
        assert!(admitted(&s, &parse_head(valid.as_bytes()).unwrap()));
        for bad in [
            valid.replace("test-only-access", "wrong"),
            valid.replace("192.168.1.10", "attacker.invalid"),
            format!("{valid}\r\nOrigin: http://attacker.invalid"),
        ] {
            assert!(!admitted(&s, &parse_head(bad.as_bytes()).unwrap()));
        }
        s.active.store(false, Ordering::SeqCst);
        assert!(!admitted(&s, &parse_head(valid.as_bytes()).unwrap()));
    }
}
