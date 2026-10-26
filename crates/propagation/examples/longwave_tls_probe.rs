//! Read-only TLS diagnostic: never use an operator token or create a contact.
//! Arguments: URL, local public-certificate trust file (or -), expected failure.

use propagation::longwave::{pinned_certificate_from_file, Failure, Longwave};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        return Err("expected URL, trust file (or -), and credentials/certificate".into());
    }
    let certificate = if args[2] == "-" {
        None
    } else {
        pinned_certificate_from_file(&args[1], std::path::Path::new(&args[2]))?
    };
    let token = format!("local-tls-check-{}", std::process::id());
    let result = Longwave::with_pinned_certificate(&args[1], &token, certificate.as_deref())
        .and_then(|client| client.operator());
    let expected = match args[3].as_str() {
        "credentials" => Failure::Credentials,
        "certificate" => Failure::Certificate,
        _ => return Err("unknown expected result".into()),
    };
    match result {
        Err(error) if error == expected => println!("PASS: {expected}"),
        Err(error) => return Err(format!("expected {expected}; got {error}").into()),
        Ok(_) => return Err("unexpected anonymous API access".into()),
    }
    Ok(())
}
