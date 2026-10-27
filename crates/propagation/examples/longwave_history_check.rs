//! Offline response check: JSON contacts on stdin, counts only on stdout.
//! Never reads credentials, contacts a server, or writes a logbook.

use propagation::longwave::RemoteContact;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let contacts: Vec<RemoteContact> =
        serde_json::from_reader(std::io::stdin()).map_err(|_| "Invalid Longwave contacts JSON")?;
    for (index, contact) in contacts.iter().enumerate() {
        contact
            .adif_record()
            .map_err(|error| format!("Contact {}: {error}", index + 1))?;
    }
    println!("PASS: {} contacts can be converted to ADIF", contacts.len());
    Ok(())
}
