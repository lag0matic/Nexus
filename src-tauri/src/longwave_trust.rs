//! Read local, operator-approved Longwave certificates. Not part of Settings,
//! profile exports or any remote write command; no automatic trust on first use.
//! The file contains only public certificates, never an API token or private key.

use propagation::longwave::{pinned_certificate_from_file, Failure, Longwave};

pub(super) fn client(url: &str, token: &str) -> Result<Longwave, Failure> {
    let path = crate::config_dir_for(None).join("longwave-trust.json");
    let certificate = pinned_certificate_from_file(url, &path)?;
    Longwave::with_pinned_certificate(url, token, certificate.as_deref())
}
