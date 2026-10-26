//! Loopback-only contract tests; no operator settings, keychain or live logbook.

use super::*;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver};
use std::thread::JoinHandle;

struct Server {
    url: String,
    received: Receiver<String>,
    thread: JoinHandle<()>,
}

impl Server {
    fn new(status: u16, body: &str, extra: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, received) = channel();
        let body = body.to_owned();
        let extra = extra.to_owned();
        let thread = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(std::time::Instant::now() < deadline, "no request received");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("loopback accept: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                request.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut bytes = vec![0; length];
            reader.read_exact(&mut bytes).unwrap();
            request.push_str(std::str::from_utf8(&bytes).unwrap());
            tx.send(request).unwrap();
            if status == 0 {
                // Simulate a commit followed by a lost HTTP reply. Keep listening
                // briefly so an accidental automatic retry is observable.
                drop(reader);
                drop(stream);
                std::thread::sleep(Duration::from_millis(200));
                assert_eq!(
                    listener.accept().unwrap_err().kind(),
                    std::io::ErrorKind::WouldBlock
                );
                return;
            }
            write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}", body.len()).unwrap();
        });
        Self {
            url,
            received,
            thread,
        }
    }

    fn finish(self) -> String {
        let request = self.received.recv_timeout(Duration::from_secs(6)).unwrap();
        self.thread.join().unwrap();
        request
    }
}

// Generated per test, not a stored credential or a copy of a real operator key.
fn token() -> String {
    format!(
        "test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

fn qso() -> Qso<'static> {
    Qso {
        station_call: " w1aw ",
        operator_call: " n0call/p ",
        band: "20M",
        mode: "cw",
        frequency_mhz: 14.0475,
        when_unix: 1_704_153_599,
        time_known: true,
    }
}

#[test]
fn qso_uses_utc_khz_and_the_correct_station_identity() {
    let mut contact = Contact::from_qso("book-a", &qso()).unwrap();
    contact.rst_sent = Some("599".into());
    contact.rst_recvd = Some("579".into());
    contact.name = Some("Quoted \"name\"\nsecond line".into());
    let body = serde_json::to_value(&contact).unwrap();
    assert_eq!(body["station_callsign"], "W1AW");
    assert_eq!(body["operator_callsign"], "N0CALL/P");
    assert_eq!(body["qso_date"], "20240101");
    assert_eq!(body["time_on"], "235959");
    assert_eq!(body["band"], "20m");
    assert_eq!(body["mode"], "CW");
    assert!((body["frequency_khz"].as_f64().unwrap() - 14047.5).abs() < 0.000001);
    assert_eq!(body["rst_recvd"], "579");
    assert_eq!(body["name"], contact.name.unwrap());
    assert!(body.get("qrz_upload_status").is_none());
    assert!(body.get("source_spot_id").is_none());
    assert!(body.get("tx_power").is_none());
    let mut next = qso();
    next.when_unix += 1;
    let next = serde_json::to_value(Contact::from_qso("book-a", &next).unwrap()).unwrap();
    assert_eq!(next["qso_date"], "20240102");
    assert_eq!(next["time_on"], "000000");
}

#[test]
fn missing_fields_unknown_time_and_nonfinite_frequencies_are_refused() {
    assert!(
        Contact::from_qso("book-a", &qso()).is_ok(),
        "positive control"
    );
    for freq in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::MAX] {
        let mut qso = qso();
        qso.frequency_mhz = freq;
        assert_eq!(
            Contact::from_qso("book-a", &qso).unwrap_err(),
            Failure::Configuration
        );
    }
    let mut unknown = qso();
    unknown.time_known = false;
    assert_eq!(
        Contact::from_qso("book-a", &unknown).unwrap_err(),
        Failure::Configuration
    );
    unknown.time_known = true;
    unknown.when_unix = u64::MAX;
    assert!(Contact::from_qso("book-a", &unknown).is_err());
    assert!(Contact::from_qso(" ", &qso()).is_err());
    let mut missing = qso();
    missing.station_call = " ";
    assert!(Contact::from_qso("book-a", &missing).is_err());
}

#[test]
fn urls_normalize_once_and_keep_reverse_proxy_prefixes() {
    for (base, expected) in [
        ("https://example.invalid", "https://example.invalid/api/v1/"),
        (
            "https://example.invalid/api/v1/",
            "https://example.invalid/api/v1/",
        ),
        (
            "https://example.invalid/longwave/",
            "https://example.invalid/longwave/api/v1/",
        ),
    ] {
        let client = Longwave::new(base, &token()).unwrap();
        assert_eq!(client.base.as_str(), expected);
    }
}

#[test]
fn unsafe_urls_and_header_injection_are_refused_before_io() {
    for base in [
        "ftp://example.invalid",
        "http://example.invalid",
        "http://8.8.8.8",
        "http://0.0.0.0",
        "https://name:password@example.invalid",
        "https://example.invalid?key=hidden",
        "https://example.invalid/#fragment",
        "",
    ] {
        assert!(matches!(
            Longwave::new(base, &token()),
            Err(Failure::Configuration)
        ));
    }
    for key in ["", " ", "header\r\nInjected: yes"] {
        assert!(matches!(
            Longwave::new("https://example.invalid", key),
            Err(Failure::Configuration)
        ));
    }
    for base in [
        "http://127.0.0.1",
        "http://192.168.1.10:8000",
        "http://[::1]:8000",
    ] {
        assert!(
            Longwave::new(base, &token()).is_ok(),
            "local positive control"
        );
    }
}

#[test]
fn account_lookup_sends_only_a_sensitive_header_and_the_correct_get_path() {
    let server = Server::new(
        200,
        r#"{"id":"operator-a","username":"operator","callsign":"N0CALL"}"#,
        "",
    );
    let key = token();
    let client = Longwave::new(&format!("{}/proxy/api/v1/", server.url), &key).unwrap();
    assert_eq!(client.operator().unwrap().callsign, "N0CALL");
    let request = server.finish();
    assert!(request.starts_with("GET /proxy/api/v1/me HTTP/1.1\r\n"));
    assert!(request.lines().any(|l| l == format!("x-api-key: {key}")));
    assert!(!request.lines().next().unwrap().contains(&key));
}

#[test]
fn logbook_listing_preserves_string_ids_and_allows_an_empty_account() {
    for body in [
        "[]",
        r#"[{"id":"book-uuid","name":"Home","operator_callsign":"N0CALL","contact_count":3}]"#,
    ] {
        let server = Server::new(200, body, "");
        let books = Longwave::new(&server.url, &token())
            .unwrap()
            .logbooks()
            .unwrap();
        if body != "[]" {
            assert_eq!(books[0].id, "book-uuid");
            assert_eq!(books[0].contact_count, 3);
        } else {
            assert!(books.is_empty());
        }
        assert!(server.finish().starts_with("GET /api/v1/logbooks "));
    }
}

#[test]
fn upload_posts_one_json_contact_and_requires_a_created_receipt() {
    let server = Server::new(
        201,
        r#"{"id":"contact-a","logbook_id":"book-a","station_callsign":"W1AW"}"#,
        "",
    );
    let client = Longwave::new(&server.url, &token()).unwrap();
    let contact = Contact::from_qso("book-a", &qso()).unwrap();
    assert_eq!(client.upload(&contact).unwrap(), "contact-a");
    let request = server.finish();
    assert!(request.starts_with("POST /api/v1/contacts "));
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["logbook_id"], "book-a");
    assert_eq!(body["frequency_khz"], 14047.5);
}

#[test]
fn a_lost_upload_reply_is_unconfirmed_and_is_never_reposted() {
    let server = Server::new(0, "", "");
    let client = Longwave::new(&server.url, &token()).unwrap();
    assert_eq!(
        client.upload(&Contact::from_qso("book-a", &qso()).unwrap()),
        Err(Failure::UnconfirmedUpload)
    );
    assert!(server.finish().starts_with("POST /api/v1/contacts "));
}

#[test]
fn refused_and_ambiguous_uploads_have_distinct_safe_errors() {
    for (status, expected) in [
        (401, Failure::Credentials),
        (403, Failure::Credentials),
        (404, Failure::NotFound),
        (422, Failure::Refused),
        (500, Failure::UnconfirmedUpload),
        (200, Failure::UnconfirmedUpload),
        (302, Failure::UnconfirmedUpload),
        (201, Failure::UnconfirmedUpload),
    ] {
        let key = token();
        let server = Server::new(status, &format!("secret echoed: {key}"), "");
        let client = Longwave::new(&server.url, &key).unwrap();
        let err = client
            .upload(&Contact::from_qso("book-a", &qso()).unwrap())
            .unwrap_err();
        assert_eq!(err, expected);
        assert!(!err.to_string().contains(&key));
        assert!(!format!("{err:?}").contains(&key));
        server.finish();
    }
}

#[test]
fn wrong_logbook_or_missing_contact_id_does_not_claim_upload_success() {
    for body in [
        r#"{"id":"contact-a","logbook_id":"other","station_callsign":"W1AW"}"#,
        r#"{"id":"","logbook_id":"book-a","station_callsign":"W1AW"}"#,
        r#"{"id":"contact-a","logbook_id":"book-a","station_callsign":"OTHER"}"#,
    ] {
        let server = Server::new(201, body, "");
        let client = Longwave::new(&server.url, &token()).unwrap();
        assert_eq!(
            client.upload(&Contact::from_qso("book-a", &qso()).unwrap()),
            Err(Failure::UnconfirmedUpload)
        );
        server.finish();
    }
}

#[test]
fn redirects_never_receive_the_token() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    target.set_nonblocking(true).unwrap();
    let redirect = format!(
        "Location: http://{}/stolen\r\n",
        target.local_addr().unwrap()
    );
    let server = Server::new(302, "", &redirect);
    let client = Longwave::new(&server.url, &token()).unwrap();
    assert_eq!(client.operator(), Err(Failure::Refused));
    server.finish();
    assert_eq!(
        target.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn oversized_and_non_api_responses_are_rejected() {
    for body in [
        "<html>sign in</html>".to_owned(),
        " ".repeat(RESPONSE_LIMIT as usize + 1),
        "{}".into(),
    ] {
        let server = Server::new(200, &body, "");
        let client = Longwave::new(&server.url, &token()).unwrap();
        assert_eq!(client.operator(), Err(Failure::InvalidResponse));
        server.finish();
    }
}
