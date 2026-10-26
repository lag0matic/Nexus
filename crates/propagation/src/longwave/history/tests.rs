//! Synthetic loopback snapshots only. No keychain, saved connection, or live server.

use super::*;
use crate::longwave::tests::token;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

struct SnapshotServer {
    url: String,
    thread: JoinHandle<Vec<String>>,
}

impl SnapshotServer {
    fn new(replies: Vec<(u16, String, Option<u64>)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body, declared_length) in replies {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "missing snapshot request");
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        Err(e) => panic!("loopback accept: {e}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    request.push_str(&line);
                    if line == "\r\n" {
                        break;
                    }
                }
                assert!(request.starts_with("GET "), "history must never write");
                requests.push(request);
                if status == 0 {
                    continue;
                }
                let length = declared_length.unwrap_or(body.len() as u64);
                let header = format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n");
                // Oversize readers intentionally close before consuming the body.
                let _ = stream.write_all(header.as_bytes());
                let _ = stream.write_all(body.as_bytes());
            }
            std::thread::sleep(Duration::from_millis(30));
            assert_eq!(
                listener.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock,
                "history must not retry or fetch guessed pages"
            );
            requests
        });
        Self { url, thread }
    }

    fn finish(self) -> Vec<String> {
        self.thread.join().unwrap()
    }
}

fn remote() -> Value {
    json!({
        "id":"contact-a", "logbook_id":"book-a", "station_callsign":"W1AW",
        "operator_callsign":"N0CALL/P", "qso_date":"20240229", "time_on":"123456",
        "band":"20m", "mode":"SSB", "frequency_khz":14250.5,
        "created_at":"2024-02-29T12:34:56Z", "updated_at":"2024-03-01T01:02:03.123456",
        "name":null, "lat":null, "lon":null
    })
}

fn books(id: &str, count: usize) -> String {
    json!([{"id":id,"name":"Selected","operator_callsign":"N0CALL","contact_count":count}])
        .to_string()
}

fn read(body: &str, count: usize) -> Result<Vec<RemoteContact>, Failure> {
    let server = SnapshotServer::new(vec![
        (200, books("book-a", count), None),
        (200, body.into(), None),
    ]);
    let client = Longwave::new(&server.url, &token()).unwrap();
    let result = client.contacts("book-a");
    let requests = server.finish();
    assert!(requests[0].starts_with("GET /api/v1/logbooks "));
    assert!(requests[1].starts_with("GET /api/v1/contacts?logbook_id=book-a "));
    result
}

#[test]
fn selected_saved_log_returns_every_record_and_empty_is_valid() {
    let mut other = remote();
    other["id"] = json!("contact-b");
    let rows = read(&json!([remote(), other]).to_string(), 2).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].id, "contact-b");
    assert!(read("[]", 0).unwrap().is_empty());
}

#[test]
fn query_encodes_selected_id_without_path_or_parameter_injection() {
    let id = "book /?&=#%+unicode-\u{e9}";
    let mut row = remote();
    row["logbook_id"] = json!(id);
    let server = SnapshotServer::new(vec![
        (200, books(id, 1), None),
        (200, json!([row]).to_string(), None),
    ]);
    let client = Longwave::new(&format!("{}/proxy", server.url), &token()).unwrap();
    assert_eq!(client.contacts(id).unwrap()[0].logbook_id, id);
    let requests = server.finish();
    let target = requests[1].split_whitespace().nth(1).unwrap();
    let parsed = reqwest::Url::parse(&format!("http://127.0.0.1{target}")).unwrap();
    assert_eq!(parsed.path(), "/proxy/api/v1/contacts");
    assert_eq!(
        parsed.query_pairs().collect::<Vec<_>>(),
        vec![("logbook_id".into(), id.into())]
    );
    assert!(parsed.fragment().is_none());
}

#[test]
fn missing_selected_log_never_fetches_contacts() {
    let server = SnapshotServer::new(vec![(200, books("other", 8), None)]);
    let client = Longwave::new(&server.url, &token()).unwrap();
    assert_eq!(client.contacts("book-a"), Err(Failure::NotFound));
    assert_eq!(server.finish().len(), 1);
    let client = Longwave::new("http://127.0.0.1:1", &token()).unwrap();
    assert_eq!(client.contacts(" \t"), Err(Failure::Configuration));
}

#[test]
fn count_is_required_and_logbooks_must_have_unique_valid_identity() {
    let book: Value = serde_json::from_str(&books("book-a", 1)).unwrap();
    for body in [
        "{}".into(),
        "null".into(),
        json!([{"id":"book-a","name":"Selected","operator_callsign":"N0CALL"}]).to_string(),
        json!([book[0], book[0]]).to_string(),
        books("", 0),
    ] {
        let server = SnapshotServer::new(vec![(200, body, None)]);
        let client = Longwave::new(&server.url, &token()).unwrap();
        assert_eq!(client.contacts("book-a"), Err(Failure::InvalidResponse));
        server.finish();
    }
}

#[test]
fn shape_count_duplicates_and_wrong_log_fail_the_entire_snapshot() {
    let good = remote();
    let mut wrong = good.clone();
    wrong["logbook_id"] = json!("other");
    let mut invalid = good.clone();
    invalid["time_on"] = json!("250000");
    let mut missing = good.clone();
    missing.as_object_mut().unwrap().remove("station_callsign");
    for (body, count) in [
        ("{}".into(), 0),
        ("null".into(), 0),
        (json!({"contacts":[good],"next":"page2"}).to_string(), 1),
        ("[]".into(), 1),
        (json!([good]).to_string(), 0),
        (json!([good, good]).to_string(), 2),
        (json!([good, wrong]).to_string(), 2),
        (json!([good, invalid]).to_string(), 2),
        (json!([good, missing]).to_string(), 2),
        ("[{".into(), 1),
        ("<html>login</html>".into(), 0),
    ] {
        assert_eq!(read(&body, count), Err(Failure::InvalidResponse));
    }
}

#[test]
fn transport_errors_fail_atomically_without_retries() {
    for (status, expected) in [
        (401, Failure::Credentials),
        (404, Failure::NotFound),
        (500, Failure::Refused),
        (206, Failure::Refused),
        (302, Failure::Refused),
        (0, Failure::Unreachable),
    ] {
        let server = SnapshotServer::new(vec![
            (200, books("book-a", 1), None),
            (status, String::new(), None),
        ]);
        let client = Longwave::new(&server.url, &token()).unwrap();
        assert_eq!(client.contacts("book-a"), Err(expected));
        server.finish();
    }
    let server = SnapshotServer::new(vec![
        (200, books("book-a", 1), None),
        (200, "[]".into(), Some(200)),
    ]);
    let client = Longwave::new(&server.url, &token()).unwrap();
    assert_eq!(client.contacts("book-a"), Err(Failure::InvalidResponse));
    server.finish();
}

#[test]
fn history_has_a_separate_bounded_limit_larger_than_upload_receipts() {
    let mut row = remote();
    row["name"] = json!("a".repeat(super::super::RESPONSE_LIMIT as usize + 1));
    assert!(read(&json!([row]).to_string(), 1).is_ok());
    assert_eq!(HISTORY_RESPONSE_LIMIT, 64 * 1024 * 1024);
    let server = SnapshotServer::new(vec![
        (200, books("book-a", 1), None),
        (200, String::new(), Some(HISTORY_RESPONSE_LIMIT + 1)),
    ]);
    let client = Longwave::new(&server.url, &token()).unwrap();
    assert_eq!(client.contacts("book-a"), Err(Failure::InvalidResponse));
    server.finish();
}

// Decode by declared UTF-8 byte lengths, as Nexus's ADIF tokenizer does. This
// checks field framing, including embedded EOR text, rather than substrings.
fn fields(mut text: &str) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    while !text.starts_with("<EOR>\n") {
        let (spec, rest) = text.strip_prefix('<').unwrap().split_once('>').unwrap();
        let (tag, len) = spec.split_once(':').unwrap();
        let (value, tail) = rest.split_at(len.parse().unwrap());
        fields.push((tag.into(), value.into()));
        text = tail;
    }
    assert_eq!(text, "<EOR>\n");
    fields
}

#[test]
fn pure_adif_conversion_preserves_metadata_units_and_inert_field_text() {
    let mut value = remote();
    value["name"] = json!("Ren\u{e9} <EOR><CALL:4>EVIL\nline");
    value["park_reference"] = json!("US-0001");
    value["lat"] = json!(39.5);
    value["lon"] = json!(-86.25);
    value["rst_sent"] = json!("59");
    value["qrz_upload_status"] = json!("Y");
    let row: RemoteContact = serde_json::from_value(value.clone()).unwrap();
    let record = row.adif_record().unwrap();
    let fields = fields(&record)
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(fields["CALL"], "W1AW");
    assert_eq!(fields["OPERATOR"], "N0CALL/P");
    assert_eq!(fields["FREQ"], "14.2505");
    assert_eq!(fields["NAME"], value["name"].as_str().unwrap());
    assert_eq!(fields[CONTACT_ID_TAG], "contact-a");
    assert_eq!(fields[LOGBOOK_ID_TAG], "book-a");
    assert_eq!(fields[SOURCE_TAG], "Longwave");
    assert_eq!(fields["APP_NEXUS_LONGWAVE_UPDATED"], row.updated_at);
    assert_eq!(fields["APP_NEXUS_LONGWAVE_LAT"], "39.5");
    assert_eq!(fields["APP_NEXUS_LONGWAVE_LON"], "-86.25");
    assert!(!fields.contains_key("LAT"));
    assert!(!fields.contains_key("LON"));
    assert!(!fields.contains_key("GRIDSQUARE"));
    assert!(!fields.contains_key("APP_NEXUS_ID"));
    assert!(!fields.keys().any(|k| k.starts_with("APP_TEMPO_UL_")));
}

#[test]
fn dates_times_and_required_fields_are_validated_without_fabrication() {
    for date in ["20240229", "20000229", "19700101", "99991231"] {
        assert!(valid_date(date));
    }
    for date in [
        "20230229",
        "21000229",
        "20241301",
        "20240431",
        "20240001",
        "20240100",
        "19691231",
        "",
        "2024-01-01",
        "2024010\u{e9}",
    ] {
        assert!(!valid_date(date), "{date}");
    }
    for time in ["0000", "2359", "000000", "235959"] {
        assert!(valid_time(time));
    }
    for time in [
        "",
        "24:00",
        "2400",
        "1260",
        "235960",
        "12345",
        "1234567",
        "12\u{e9}0",
    ] {
        assert!(!valid_time(time), "{time}");
    }
    for key in [
        "id",
        "logbook_id",
        "station_callsign",
        "operator_callsign",
        "band",
        "mode",
        "qso_date",
        "time_on",
        "created_at",
        "updated_at",
    ] {
        let mut value = remote();
        value[key] = json!("");
        let row: RemoteContact = serde_json::from_value(value).unwrap();
        assert_eq!(row.adif_record(), Err(Failure::InvalidResponse), "{key}");
    }
    for (key, bad) in [
        ("frequency_khz", json!(0)),
        ("lat", json!(91)),
        ("lon", json!(-181)),
        ("qrz_upload_date", json!("20240230")),
        ("name", json!("bad\u{0000}value")),
    ] {
        let mut value = remote();
        value[key] = bad;
        let row: RemoteContact = serde_json::from_value(value).unwrap();
        assert_eq!(row.adif_record(), Err(Failure::InvalidResponse));
    }
    let mut row: RemoteContact = serde_json::from_value(remote()).unwrap();
    row.frequency_khz = f64::NAN;
    assert!(row.adif_record().is_err());
}

#[test]
fn only_explicit_grid_is_exported_as_gridsquare() {
    let mut value = remote();
    value["lat"] = json!(39.5);
    value["lon"] = json!(-86.25);
    value["grid_square"] = json!("EM79");
    let row: RemoteContact = serde_json::from_value(value).unwrap();
    let fields = fields(&row.adif_record().unwrap())
        .into_iter()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(fields["GRIDSQUARE"], "EM79");
}

#[test]
fn header_terminator_inside_values_is_rejected_before_import() {
    for value in ["<EOH>", "text<eOh>text"] {
        let mut row: RemoteContact = serde_json::from_value(remote()).unwrap();
        row.name = Some(value.into());
        assert_eq!(row.adif_record(), Err(Failure::InvalidResponse));
    }
}

#[test]
fn streaming_response_limit_is_enforced_without_content_length() {
    for (body, expected) in [("[]              ", true), ("[]               ", false)] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
            }
            write!(
                stream,
                "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{body}"
            )
            .unwrap();
        });
        let client = Longwave::new(&url, &token()).unwrap();
        let result: Result<Vec<RemoteContact>, _> =
            client.request_url(Method::GET, client.base.join("contacts").unwrap(), None, 16);
        assert_eq!(result.is_ok(), expected);
        if !expected {
            assert_eq!(result, Err(Failure::InvalidResponse));
        }
        thread.join().unwrap();
    }
}
