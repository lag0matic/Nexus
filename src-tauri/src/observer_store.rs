//! Browser-panel pairing only. The OS vault keeps the shared receive-only bearer
//! and startup preference, separate from Remote identities and radio settings.
//! An unreadable record is never replaced. There is no plaintext-file fallback.
use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Saved {
    pub access: String,
    pub automatic: bool,
    pub address: Ipv4Addr,
    pub port: u16,
}

impl Saved {
    pub fn valid(&self) -> bool {
        self.access.len() == 64
            && self
                .access
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            && tempo_stream::lan::listenable(self.address)
            && self.port >= 1024
    }
}

fn entry() -> Result<keyring::Entry, String> {
    // Profiles must never restore each other's browser access or startup setting.
    let path = crate::config_dir();
    let hash = ring::digest::digest(&ring::digest::SHA256, path.to_string_lossy().as_bytes());
    let account: String = hash.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    keyring::Entry::new("org.hamradiotools.nexus.browser-panels", &account)
        .map_err(|_| unavailable())
}

fn unavailable() -> String {
    "Browser pairing storage unavailable. Unlock the OS credential store; automatic startup is disabled.".into()
}

pub(super) fn load() -> Result<Option<Saved>, String> {
    match entry()?.get_password() {
        Ok(text) => {
            let saved: Saved = serde_json::from_str(&text).map_err(|_| unavailable())?;
            if !saved.valid() {
                return Err(unavailable());
            }
            Ok(Some(saved))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(unavailable()),
    }
}

pub(super) fn save(saved: &Saved) -> Result<(), String> {
    if !saved.valid() {
        return Err("Invalid browser sharing settings".into());
    }
    let text = serde_json::to_string(saved).map_err(|_| unavailable())?;
    entry()?.set_password(&text).map_err(|_| unavailable())
}

pub(super) fn clear() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(unavailable()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated, unlocked OS Secret Service; never use the operator's vault"]
    fn observer_os_vault_persists_startup_and_pairing_and_refuses_corrupt_records() {
        // config_dir() is test-private. Run in a fresh dbus-run-session as well,
        // so neither the name nor the service can reach a real profile's secret.
        clear().unwrap();
        assert!(load().unwrap().is_none());
        let mut bytes = [0u8; 32];
        ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes).unwrap();
        let saved = Saved {
            access: bytes.iter().map(|b| format!("{b:02x}")).collect(),
            automatic: true,
            address: "192.168.1.10".parse().unwrap(),
            port: 7375,
        };
        save(&saved).unwrap();
        let restored = load().unwrap().unwrap();
        assert_eq!(restored.access, saved.access);
        assert!(restored.automatic);
        assert_eq!(restored.address, saved.address);
        entry().unwrap().set_password("corrupt record").unwrap();
        assert!(load().is_err());
        assert_eq!(entry().unwrap().get_password().unwrap(), "corrupt record");
        clear().unwrap();
        assert!(load().unwrap().is_none());
    }

    #[test]
    fn observer_saved_pairing_roundtrips_without_granting_transmit_or_default_startup() {
        let saved = Saved {
            access: "a".repeat(64),
            automatic: false,
            address: "192.168.1.10".parse().unwrap(),
            port: 7375,
        };
        let text = serde_json::to_string(&saved).unwrap();
        let restored: Saved = serde_json::from_str(&text).unwrap();
        assert!(restored.valid());
        assert_eq!(saved.access, restored.access);
        assert!(!restored.automatic);
        let mut invalid = restored.clone();
        invalid.address = Ipv4Addr::UNSPECIFIED;
        assert!(!invalid.valid());
        invalid = restored.clone();
        invalid.port = 80;
        assert!(!invalid.valid());
        invalid = restored;
        invalid.access = "not a credential".into();
        assert!(!invalid.valid());
        assert!(serde_json::from_str::<Saved>(&text.replace(
            "\"automatic\":false",
            "\"automatic\":false,\"transmit\":true"
        ))
        .is_err());
    }
}
