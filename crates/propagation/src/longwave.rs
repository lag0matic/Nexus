//! Longwave's `/api/v1` logging API. Blocking I/O belongs on the connector worker,
//! never under the Engine mutex. The caller supplies a token from the OS keychain;
//! this adapter neither stores it nor includes remote text in diagnostics.
//!
//! Longwave's POST is NOT idempotent. An unconfirmed upload must not be retried
//! automatically: the server may already have committed it. Reads are harmless.
//! HTTPS verifies certificates normally unless the operator has explicitly pinned
//! this origin's exact certificate. Plain HTTP is restricted to literal
//! private/loopback addresses, with no proxy or redirect; LAN names need HTTPS.

use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Method, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::Read;
use std::net::IpAddr;
use std::time::Duration;

mod tls;
pub use tls::{pinned_certificate, pinned_certificate_from_file};
mod history;
pub use history::{RemoteContact, CONTACT_ID_TAG, LOGBOOK_ID_TAG, SOURCE_TAG};

const RESPONSE_LIMIT: u64 = 1024 * 1024;

/// Closed, safe diagnostic vocabulary. No URL, token, response or parser text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Configuration,
    Credentials,
    NotFound,
    Refused,
    Unreachable,
    Certificate,
    InvalidResponse,
    /// A submission may have been saved. Inspect Longwave before sending again.
    UnconfirmedUpload,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Configuration => "Check the Longwave URL, client token and contact fields",
            Self::Credentials => "Longwave refused the client token",
            Self::NotFound => "Longwave API or selected logbook was not found",
            Self::Refused => "Longwave refused the request",
            Self::Unreachable => "Longwave could not be reached",
            Self::Certificate => "Longwave HTTPS certificate was not trusted; check this server's certificate approval",
            Self::InvalidResponse => "Longwave did not return the expected API response",
            Self::UnconfirmedUpload => {
                "Longwave upload is unconfirmed; check the logbook before sending it again"
            }
        })
    }
}

impl std::error::Error for Failure {}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Operator {
    pub id: String,
    pub username: String,
    pub callsign: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Logbook {
    pub id: String,
    pub name: String,
    pub operator_callsign: String,
    pub park_reference: Option<String>,
    #[serde(default)]
    pub contact_count: u64,
}

/// Snapshot of the fields Longwave can represent. `station_call` is the station
/// WORKED, not our call; `operator_call` is our station/operator identity.
pub struct Qso<'a> {
    pub station_call: &'a str,
    pub operator_call: &'a str,
    pub band: &'a str,
    pub mode: &'a str,
    pub frequency_mhz: f64,
    pub when_unix: u64,
    pub time_known: bool,
}

/// Longwave's snake_case contact-create schema. Unsupported ADIF metadata stays
/// in Nexus's authoritative local log rather than being fabricated or discarded there.
#[derive(Clone, Debug, Serialize)]
pub struct Contact {
    logbook_id: String,
    station_callsign: String,
    operator_callsign: String,
    qso_date: String,
    time_on: String,
    band: String,
    mode: String,
    frequency_khz: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rst_sent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rst_recvd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_power: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qth: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub county: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_square: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dxcc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub park_reference: Option<String>,
}

impl Contact {
    pub fn from_qso(logbook_id: &str, qso: &Qso<'_>) -> Result<Self, Failure> {
        if !qso.time_known || qso.when_unix > 253_402_300_799 {
            return Err(Failure::Configuration);
        }
        let (y, m, d, h, min, sec) = tempo_base::datetime_utc(qso.when_unix);
        let contact = Self {
            logbook_id: logbook_id.trim().into(),
            station_callsign: qso.station_call.trim().to_ascii_uppercase(),
            operator_callsign: qso.operator_call.trim().to_ascii_uppercase(),
            // Longwave stores these verbatim and writes them directly into ADIF.
            qso_date: format!("{y:04}{m:02}{d:02}"),
            time_on: format!("{h:02}{min:02}{sec:02}"),
            band: qso.band.trim().to_ascii_lowercase(),
            mode: qso.mode.trim().to_ascii_uppercase(),
            frequency_khz: qso.frequency_mhz * 1000.0,
            rst_sent: None,
            rst_recvd: None,
            tx_power: None,
            name: None,
            qth: None,
            county: None,
            grid_square: None,
            country: None,
            state: None,
            dxcc: None,
            park_reference: None,
        };
        contact.validate()?;
        Ok(contact)
    }

    fn validate(&self) -> Result<(), Failure> {
        if [
            &self.logbook_id,
            &self.station_callsign,
            &self.operator_callsign,
            &self.band,
            &self.mode,
            &self.qso_date,
            &self.time_on,
        ]
        .iter()
        .any(|s| s.trim().is_empty())
            || !self.frequency_khz.is_finite()
            || self.frequency_khz <= 0.0
        {
            return Err(Failure::Configuration);
        }
        Ok(())
    }
}

/// Deliberately not Debug: the HTTP client's default headers contain a credential.
pub struct Longwave {
    client: Client,
    base: Url,
}

impl Longwave {
    pub fn new(base: &str, client_token: &str) -> Result<Self, Failure> {
        Self::with_pinned_certificate(base, client_token, None)
    }

    /// A locally approved exact leaf certificate, never fetched or trusted automatically.
    pub fn with_pinned_certificate(
        base: &str,
        client_token: &str,
        certificate: Option<&[u8]>,
    ) -> Result<Self, Failure> {
        let mut base = Url::parse(base.trim()).map_err(|_| Failure::Configuration)?;
        if !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || base.host_str().is_none()
        {
            return Err(Failure::Configuration);
        }
        let plain = base.scheme() == "http";
        if base.scheme() != "https" && !plain {
            return Err(Failure::Configuration);
        }
        if plain {
            let host = base.host_str().unwrap_or_default().trim_matches(['[', ']']);
            let ip: IpAddr = host.parse().map_err(|_| Failure::Configuration)?;
            let local = match ip {
                IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
                IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local(),
            };
            if !local {
                return Err(Failure::Configuration);
            }
        }
        let path = base.path().trim_end_matches('/');
        let path = if path.ends_with("/api/v1") {
            format!("{path}/")
        } else {
            format!("{path}/api/v1/")
        };
        base.set_path(&path);
        let token = client_token.trim();
        if token.is_empty() {
            return Err(Failure::Configuration);
        }
        let mut token = HeaderValue::from_str(token).map_err(|_| Failure::Configuration)?;
        token.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert("X-Api-Key", token);
        let mut client = Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .https_only(!plain)
            .default_headers(headers);
        if let Some(certificate) = certificate {
            if plain {
                return Err(Failure::Configuration);
            }
            client = client.use_preconfigured_tls(tls::config(&base, certificate)?);
        }
        let client = client.build().map_err(|_| Failure::Configuration)?;
        Ok(Self { client, base })
    }

    /// Read-only connection test; does not create logbooks or contacts.
    pub fn operator(&self) -> Result<Operator, Failure> {
        let operator: Operator = self.request(Method::GET, "me", None)?;
        if operator.id.trim().is_empty() || operator.callsign.trim().is_empty() {
            return Err(Failure::InvalidResponse);
        }
        Ok(operator)
    }

    pub fn logbooks(&self) -> Result<Vec<Logbook>, Failure> {
        let books: Vec<Logbook> = self.request(Method::GET, "logbooks", None)?;
        if books
            .iter()
            .any(|b| b.id.trim().is_empty() || b.name.trim().is_empty())
        {
            return Err(Failure::InvalidResponse);
        }
        Ok(books)
    }

    /// Send exactly once. Returns Longwave's new contact ID only after a valid 201.
    pub fn upload(&self, contact: &Contact) -> Result<String, Failure> {
        contact.validate()?;
        #[derive(Deserialize)]
        struct Receipt {
            id: String,
            logbook_id: String,
            station_callsign: String,
        }
        let receipt: Receipt = self.request(Method::POST, "contacts", Some(contact))?;
        if receipt.id.trim().is_empty()
            || receipt.logbook_id != contact.logbook_id
            || !receipt
                .station_callsign
                .eq_ignore_ascii_case(&contact.station_callsign)
        {
            return Err(Failure::UnconfirmedUpload);
        }
        Ok(receipt.id)
    }

    fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        contact: Option<&Contact>,
    ) -> Result<T, Failure> {
        let url = self.base.join(path).map_err(|_| Failure::Configuration)?;
        self.request_url(method, url, contact, RESPONSE_LIMIT)
    }

    fn request_url<T: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        contact: Option<&Contact>,
        response_limit: u64,
    ) -> Result<T, Failure> {
        let posting = contact.is_some();
        let uncertain = if posting {
            Failure::UnconfirmedUpload
        } else {
            Failure::Unreachable
        };
        let mut request = self.client.request(method, url);
        if let Some(contact) = contact {
            request = request.json(contact);
        }
        let response = request.send().map_err(|error| {
            if !posting && tls::certificate_error(&error) {
                Failure::Certificate
            } else {
                uncertain
            }
        })?;
        let status = response.status().as_u16();
        match status {
            401 | 403 => return Err(Failure::Credentials),
            404 => return Err(Failure::NotFound),
            400 | 409 | 422 => return Err(Failure::Refused),
            200 if !posting => {}
            201 if posting => {}
            _ => {
                return Err(if posting {
                    Failure::UnconfirmedUpload
                } else {
                    Failure::Refused
                })
            }
        }
        let invalid = if posting {
            Failure::UnconfirmedUpload
        } else {
            Failure::InvalidResponse
        };
        read_json(response, response_limit).map_err(|_| invalid)
    }
}

fn read_json<T: DeserializeOwned>(response: Response, limit: u64) -> Result<T, ()> {
    if response
        .content_length()
        .is_some_and(|length| length > limit)
    {
        return Err(());
    }
    let mut body = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut body)
        .map_err(|_| ())?;
    if body.len() as u64 > limit {
        return Err(());
    }
    serde_json::from_slice(&body).map_err(|_| ())
}

#[cfg(test)]
mod tests;
