//! Selected-log, read-only snapshots of Longwave's unpaginated contact API.
//! All records must be usable before any are returned. A count race fails closed;
//! callers can explicitly refresh, but must never infer local deletions from this read.

use super::{Failure, Longwave};
use reqwest::Method;
use serde::Deserialize;
use std::collections::HashSet;
use std::fmt::Write;

const HISTORY_RESPONSE_LIMIT: u64 = 64 * 1024 * 1024;
pub const CONTACT_ID_TAG: &str = "APP_NEXUS_LONGWAVE_CONTACT_ID";
pub const LOGBOOK_ID_TAG: &str = "APP_NEXUS_LONGWAVE_LOGBOOK_ID";
pub const SOURCE_TAG: &str = "APP_NEXUS_IMPORT_SOURCE";

/// Raw server identity and ADIF-shaped fields, not the contact-create payload.
/// Datetimes are retained as strings: never mistaken for epoch milliseconds or
/// used to manufacture a missing QSO time. Unknown future JSON fields are ignored.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct RemoteContact {
    pub id: String,
    pub logbook_id: String,
    pub station_callsign: String,
    pub operator_callsign: String,
    pub qso_date: String,
    pub time_on: String,
    pub band: String,
    pub mode: String,
    pub frequency_khz: f64,
    pub park_reference: Option<String>,
    pub rst_sent: Option<String>,
    pub rst_recvd: Option<String>,
    pub tx_power: Option<String>,
    pub name: Option<String>,
    pub qth: Option<String>,
    pub county: Option<String>,
    pub grid_square: Option<String>,
    pub country: Option<String>,
    pub state: Option<String>,
    pub dxcc: Option<String>,
    pub qrz_upload_status: Option<String>,
    pub qrz_upload_date: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub source_spot_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Longwave {
    /// Read the selected saved log only. Longwave has no pagination: its response
    /// is a bare array. Require the advertised count (including zero) so a valid
    /// JSON prefix or a concurrent count change cannot silently become a full read.
    /// No writes, retries, local reconciliation, or upload-acceptance stamps occur here.
    pub fn contacts(&self, logbook_id: &str) -> Result<Vec<RemoteContact>, Failure> {
        if logbook_id.trim().is_empty() {
            return Err(Failure::Configuration);
        }
        #[derive(Deserialize)]
        struct Book {
            id: String,
            name: String,
            operator_callsign: String,
            contact_count: u64,
        }
        let url = self
            .base
            .join("logbooks")
            .map_err(|_| Failure::Configuration)?;
        let books: Vec<Book> = self.request_url(Method::GET, url, None, HISTORY_RESPONSE_LIMIT)?;
        let mut ids = HashSet::new();
        for book in &books {
            if book.id.trim().is_empty()
                || book.name.trim().is_empty()
                || book.operator_callsign.trim().is_empty()
                || !ids.insert(&book.id)
            {
                return Err(Failure::InvalidResponse);
            }
        }
        let book = books
            .iter()
            .find(|book| book.id == logbook_id)
            .ok_or(Failure::NotFound)?;
        let mut url = self
            .base
            .join("contacts")
            .map_err(|_| Failure::Configuration)?;
        url.query_pairs_mut().append_pair("logbook_id", logbook_id);
        let contacts: Vec<RemoteContact> =
            self.request_url(Method::GET, url, None, HISTORY_RESPONSE_LIMIT)?;
        if contacts.len() as u64 != book.contact_count {
            return Err(Failure::InvalidResponse);
        }
        let mut ids = HashSet::new();
        for contact in &contacts {
            if contact.logbook_id != logbook_id || !ids.insert(&contact.id) {
                return Err(Failure::InvalidResponse);
            }
            contact.adif_record()?;
        }
        Ok(contacts)
    }
}

impl RemoteContact {
    /// One headerless, length-prefixed ADIF record including source identity.
    /// These are remote IDs, NOT APP_NEXUS_ID. The durable importer owns local
    /// identity, additive merge policy, and the Longwave accepted-upload stamp.
    pub fn adif_record(&self) -> Result<String, Failure> {
        if [
            &self.id,
            &self.logbook_id,
            &self.station_callsign,
            &self.operator_callsign,
            &self.band,
            &self.mode,
            &self.created_at,
            &self.updated_at,
        ]
        .iter()
        .any(|value| value.trim().is_empty())
            || !valid_date(&self.qso_date)
            || !valid_time(&self.time_on)
            || !self.frequency_khz.is_finite()
            || self.frequency_khz <= 0.0
            || self
                .lat
                .is_some_and(|v| !v.is_finite() || !(-90.0..=90.0).contains(&v))
            || self
                .lon
                .is_some_and(|v| !v.is_finite() || !(-180.0..=180.0).contains(&v))
            || self
                .qrz_upload_date
                .as_deref()
                .is_some_and(|v| !v.is_empty() && !valid_date(v))
        {
            return Err(Failure::InvalidResponse);
        }
        let mut out = String::new();
        for (tag, value) in [
            ("CALL", self.station_callsign.as_str()),
            ("OPERATOR", &self.operator_callsign),
            ("STATION_CALLSIGN", &self.operator_callsign),
            ("QSO_DATE", &self.qso_date),
            ("TIME_ON", &self.time_on),
            ("BAND", &self.band),
            ("MODE", &self.mode),
            ("FREQ", &(self.frequency_khz / 1000.0).to_string()),
            (SOURCE_TAG, "Longwave"),
            (CONTACT_ID_TAG, &self.id),
            (LOGBOOK_ID_TAG, &self.logbook_id),
            ("APP_NEXUS_LONGWAVE_CREATED_AT", &self.created_at),
            ("APP_NEXUS_LONGWAVE_UPDATED", &self.updated_at),
        ] {
            field(&mut out, tag, value)?;
        }
        for (tag, value) in [
            ("RST_SENT", &self.rst_sent),
            ("RST_RCVD", &self.rst_recvd),
            ("TX_PWR", &self.tx_power),
            ("NAME", &self.name),
            ("QTH", &self.qth),
            ("CNTY", &self.county),
            ("GRIDSQUARE", &self.grid_square),
            ("COUNTRY", &self.country),
            ("STATE", &self.state),
            ("DXCC", &self.dxcc),
            ("POTA_REF", &self.park_reference),
            ("SIG_INFO", &self.park_reference),
            ("QRZCOM_QSO_UPLOAD_STATUS", &self.qrz_upload_status),
            ("QRZCOM_QSO_UPLOAD_DATE", &self.qrz_upload_date),
            ("APP_NEXUS_LONGWAVE_SOURCE_SPOT_ID", &self.source_spot_id),
        ] {
            if let Some(value) = value {
                field(&mut out, tag, value)?;
            }
        }
        if self.park_reference.as_ref().is_some_and(|v| !v.is_empty()) {
            field(&mut out, "SIG", "POTA")?;
        }
        // Longwave coordinates may be approximate country geocodes. Preserve
        // their raw values as provenance only, never as award-eligible location.
        for (tag, value) in [
            ("APP_NEXUS_LONGWAVE_LAT", self.lat),
            ("APP_NEXUS_LONGWAVE_LON", self.lon),
        ] {
            if let Some(value) = value {
                field(&mut out, tag, &value.to_string())?;
            }
        }
        out.push_str("<EOR>\n");
        Ok(out)
    }
}

fn field(out: &mut String, tag: &str, value: &str) -> Result<(), Failure> {
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        // Core finds the header terminator before tokenizing a headerless record.
        // Refuse this one sentinel even inside a length-prefixed field value.
        || value.as_bytes().windows(5).any(|s| s.eq_ignore_ascii_case(b"<EOH>"))
    {
        return Err(Failure::InvalidResponse);
    }
    // Nexus's trusted ADIF reader consumes UTF-8 byte lengths, including quoted
    // tag-like text inside values. Never interpolate a remote value as a tag.
    write!(out, "<{tag}:{}>{value}", value.len()).map_err(|_| Failure::InvalidResponse)
}

fn valid_date(value: &str) -> bool {
    if value.len() != 8 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let year: u32 = value[..4].parse().unwrap_or(0);
    let month: usize = value[4..6].parse().unwrap_or(0);
    let day: u32 = value[6..].parse().unwrap_or(0);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = [
        0,
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    year >= 1970 && day > 0 && days.get(month).is_some_and(|max| day <= *max)
}

fn valid_time(value: &str) -> bool {
    if !matches!(value.len(), 4 | 6) || !value.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let pair = |start| value[start..start + 2].parse::<u32>().unwrap_or(u32::MAX);
    pair(0) < 24 && pair(2) < 60 && (value.len() == 4 || pair(4) < 60)
}

#[cfg(test)]
mod tests;
