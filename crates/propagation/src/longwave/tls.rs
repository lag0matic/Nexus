//! Explicit per-origin trust for a privately issued Longwave certificate. The local
//! trust file is operator-managed, not a Settings field or a remote command. Matching
//! exact DER is the identity policy for that approved origin, like a host-key pin;
//! approval lasts until revoked or the certificate changes. Rustls still
//! verifies the server's handshake signature before any HTTP headers can be sent.

use super::Failure;
use reqwest::Url;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, ClientConfig, DigitallySignedStruct, Error, SignatureScheme};
use serde::Deserialize;
use std::io::Read;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustStore {
    version: u32,
    servers: Vec<ApprovedServer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApprovedServer {
    origin: String,
    certificate_der: Vec<u8>,
}

/// Missing local approval means normal HTTPS validation, never implicit trust.
/// An unreadable, malformed or oversized approval file is a configuration error.
pub fn pinned_certificate_from_file(
    base: &str,
    path: &std::path::Path,
) -> Result<Option<Vec<u8>>, Failure> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Failure::Configuration),
    };
    certificate_from_reader(base, file)
}

fn certificate_from_reader(base: &str, reader: impl Read) -> Result<Option<Vec<u8>>, Failure> {
    let mut json = String::new();
    reader
        .take(65_537)
        .read_to_string(&mut json)
        .map_err(|_| Failure::Configuration)?;
    pinned_certificate(base, &json)
}

/// Resolve only an exact HTTPS origin (scheme, host and port). API paths do not
/// broaden trust to other hosts, schemes or ports. Duplicate approvals fail closed.
pub fn pinned_certificate(base: &str, json: &str) -> Result<Option<Vec<u8>>, Failure> {
    if json.len() > 65_536 {
        return Err(Failure::Configuration);
    }
    let store: TrustStore = serde_json::from_str(json).map_err(|_| Failure::Configuration)?;
    if store.version != 1 || store.servers.len() > 16 {
        return Err(Failure::Configuration);
    }
    let requested = Url::parse(base.trim()).map_err(|_| Failure::Configuration)?;
    let mut origins = std::collections::HashSet::new();
    let mut found = None;
    for server in store.servers {
        let origin = Url::parse(&server.origin).map_err(|_| Failure::Configuration)?;
        if origin.scheme() != "https"
            || origin.host_str().is_none()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
            || origin.path() != "/"
            || !(64..=16_384).contains(&server.certificate_der.len())
            || !origins.insert(origin.origin().ascii_serialization())
        {
            return Err(Failure::Configuration);
        }
        if requested.origin() == origin.origin() {
            found = Some(server.certificate_der);
        }
    }
    Ok(found)
}

#[derive(Debug)]
struct PinnedServer {
    certificate: Vec<u8>,
    name: ServerName<'static>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PinnedServer {
    fn verify_server_cert(
        &self,
        certificate: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if certificate.as_ref() == self.certificate && name == &self.name {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(CertificateError::ApplicationVerificationFailure.into())
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signed: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(
            message,
            certificate,
            signed,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signed: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(
            message,
            certificate,
            signed,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub(super) fn config(base: &Url, certificate: &[u8]) -> Result<ClientConfig, Failure> {
    if !(64..=16_384).contains(&certificate.len()) {
        return Err(Failure::Configuration);
    }
    let name = ServerName::try_from(
        base.host_str()
            .ok_or(Failure::Configuration)?
            .trim_matches(['[', ']'])
            .to_owned(),
    )
    .map_err(|_| Failure::Configuration)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|_| Failure::Configuration)?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedServer {
            certificate: certificate.to_vec(),
            name,
            provider,
        }))
        .with_no_client_auth();
    config.resumption = rustls::client::Resumption::disabled();
    config.enable_early_data = false;
    Ok(config)
}

pub(super) fn certificate_error(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut source = Some(error);
    while let Some(error) = source {
        if matches!(
            error.downcast_ref::<Error>(),
            Some(Error::InvalidCertificate(_))
        ) {
            return true;
        }
        // io::Error's source() can skip the wrapped error itself. Hyper/rustls
        // carries certificate failures in that inner box.
        if let Some(inner) = error
            .downcast_ref::<std::io::Error>()
            .and_then(|e| e.get_ref())
        {
            if certificate_error(inner) {
                return true;
            }
        }
        source = error.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(origins: &[&str]) -> String {
        serde_json::json!({
            "version": 1,
            "servers": origins.iter().map(|origin| serde_json::json!({
                "origin": origin,
                "certificate_der": vec![42u8; 128],
            })).collect::<Vec<_>>(),
        })
        .to_string()
    }

    #[test]
    fn wrapped_certificate_errors_are_distinct_from_network_errors() {
        let certificate =
            std::io::Error::other(Error::InvalidCertificate(CertificateError::UnknownIssuer));
        assert!(certificate_error(&certificate));
        assert!(!certificate_error(&std::io::Error::from(
            std::io::ErrorKind::ConnectionRefused
        )));
    }

    #[test]
    fn approval_file_reads_are_bounded_and_fail_closed() {
        assert!(certificate_from_reader(
            "https://localhost",
            store(&["https://localhost"]).as_bytes()
        )
        .unwrap()
        .is_some());
        assert!(certificate_from_reader("https://localhost", &b"not JSON"[..]).is_err());
        assert!(certificate_from_reader("https://localhost", &b"\xff"[..]).is_err());
        assert!(
            certificate_from_reader("https://localhost", vec![b' '; 65_537].as_slice()).is_err()
        );
        let path = std::env::temp_dir().join(format!(
            "absent-nexus-longwave-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert_eq!(
            pinned_certificate_from_file("https://localhost", &path),
            Ok(None)
        );
    }

    #[test]
    fn certificate_pins_cannot_be_used_for_plain_http() {
        assert!(super::super::Longwave::with_pinned_certificate(
            "http://127.0.0.1",
            "test",
            Some(&[42; 128])
        )
        .is_err());
        assert!(config(&Url::parse("https://localhost").unwrap(), &[]).is_err());
    }

    #[test]
    fn trust_is_limited_to_exact_https_origin() {
        let json = store(&["https://127.0.0.1"]);
        assert!(pinned_certificate("https://127.0.0.1:443/api/v1", &json)
            .unwrap()
            .is_some());
        for base in [
            "http://127.0.0.1",
            "https://127.0.0.1:444",
            "https://localhost",
            "https://127.0.0.2",
        ] {
            assert!(pinned_certificate(base, &json).unwrap().is_none(), "{base}");
        }
    }

    #[test]
    fn malformed_or_ambiguous_trust_is_refused() {
        for origins in [
            vec!["http://127.0.0.1"],
            vec!["https://user@localhost"],
            vec!["https://localhost/path"],
            vec!["https://localhost?other"],
            vec!["https://localhost#other"],
            vec!["https://localhost", "https://localhost:443"],
        ] {
            assert!(pinned_certificate("https://localhost", &store(&origins)).is_err());
        }
        for json in ["{}", "{", r#"{"version":2,"servers":[]}"#] {
            assert!(pinned_certificate("https://localhost", json).is_err());
        }
    }

    #[test]
    fn exact_certificate_and_server_name_are_both_required() {
        let name = ServerName::try_from("127.0.0.1").unwrap();
        let pin = PinnedServer {
            certificate: vec![42; 128],
            name: name.clone(),
            provider: Arc::new(rustls::crypto::ring::default_provider()),
        };
        let now = UnixTime::since_unix_epoch(std::time::Duration::from_secs(1));
        let cert = CertificateDer::from(vec![42; 128]);
        assert!(pin.verify_server_cert(&cert, &[], &name, &[], now).is_ok());
        assert!(pin
            .verify_server_cert(&CertificateDer::from(vec![43; 128]), &[], &name, &[], now)
            .is_err());
        assert!(pin
            .verify_server_cert(
                &cert,
                &[],
                &ServerName::try_from("localhost").unwrap(),
                &[],
                now
            )
            .is_err());
        assert!(!pin.supported_verify_schemes().is_empty());
        let config = config(&Url::parse("https://127.0.0.1").unwrap(), cert.as_ref()).unwrap();
        assert!(!config.enable_early_data);
    }
}
