//! Read the retired custom live-mic choice once, without restoring its TX path.
//! Explicit upstream choices win, including false, an empty device and zero gain.

use super::Settings;
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Choice {
    phone_desktop_mic: Option<bool>,
    pc_mic: Option<bool>,
    pc_mic_device: Option<String>,
    pc_mic_gain_db: Option<f32>,
    radios: Vec<RadioChoice>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct RadioChoice {
    id: u32,
    pc_mic: Option<bool>,
    pc_mic_device: Option<String>,
    pc_mic_gain_db: Option<f32>,
}

pub(super) fn migrate(settings: &mut Settings, saved: Option<Choice>) {
    let Some(saved) = saved else { return };
    let Some(legacy_on) = saved.phone_desktop_mic else {
        return;
    };
    let raw = saved.radios.iter().find(|p| p.id == settings.active_radio);
    let old_device = if legacy_on {
        settings.voice_mic_device.clone()
    } else {
        String::new()
    };
    let Some(profile) = settings
        .radios
        .iter_mut()
        .find(|p| p.id == settings.active_radio)
    else {
        return;
    };
    if raw.and_then(|p| p.pc_mic).is_none() {
        profile.pc_mic = saved.pc_mic.unwrap_or(legacy_on);
    }
    if raw.and_then(|p| p.pc_mic_device.as_ref()).is_none() {
        profile.pc_mic_device = saved.pc_mic_device.unwrap_or(old_device);
    }
    if raw.and_then(|p| p.pc_mic_gain_db).is_none() {
        profile.pc_mic_gain_db = saved.pc_mic_gain_db.unwrap_or(0.0).clamp(-20.0, 20.0);
    }
}

#[cfg(test)]
mod tests {
    use super::super::Settings;
    use serde_json::{json, Value};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn load(raw: Value) -> (Settings, std::path::PathBuf) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "nexus_pc_mic_migration_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
        (Settings::load(&path), path)
    }

    #[test]
    fn legacy_pc_mic_migrates_flat_settings_without_touching_voice_recording() {
        let (s, path) = load(
            json!({"phoneDesktopMic": true, "voiceMicDevice": "Desk microphone", "rigModel": 3073}),
        );
        assert!(s.pc_mic);
        assert_eq!(s.pc_mic_device, "Desk microphone");
        assert_eq!(s.pc_mic_gain_db, 0.0);
        assert_eq!(s.voice_mic_device, "Desk microphone");
        assert!(s.radios[0].pc_mic);
        assert!(!s.unknown.contains_key("phoneDesktopMic"));
        s.save(&path).unwrap();
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(saved.get("phoneDesktopMic").is_none());
        let again = Settings::load(&path);
        assert!(again.pc_mic);
        assert_eq!(again.pc_mic_device, s.pc_mic_device);
    }

    #[test]
    fn legacy_pc_mic_only_migrates_the_selected_radio() {
        let (s, _) = load(
            json!({"phoneDesktopMic": true, "voiceMicDevice": "Desk microphone", "activeRadio": 7,
            "radios": [{"id": 2, "name": "Other radio"}, {"id": 7, "name": "IC-7300", "rigModel": 3073}]}),
        );
        assert!(!s.radios[0].pc_mic);
        assert!(s.radios[0].pc_mic_device.is_empty());
        assert!(s.radios[1].pc_mic);
        assert_eq!(s.radios[1].pc_mic_device, "Desk microphone");
        assert!(s.pc_mic);
    }

    #[test]
    fn legacy_pc_mic_never_overwrites_an_explicit_upstream_choice() {
        for on in [false, true] {
            let (s, _) = load(
                json!({"phoneDesktopMic": true, "voiceMicDevice": "Old microphone", "activeRadio": 7,
                "radios": [{"id": 7, "pcMic": on, "pcMicDevice": "Chosen microphone", "pcMicGainDb": 6.0}]}),
            );
            assert_eq!(s.pc_mic, on);
            assert_eq!(s.pc_mic_device, "Chosen microphone");
            assert_eq!(s.pc_mic_gain_db, 6.0);
        }
        let (s, _) = load(
            json!({"phoneDesktopMic": true, "voiceMicDevice": "Old microphone",
            "pcMic": false, "pcMicDevice": "", "pcMicGainDb": -3.0}),
        );
        assert!(!s.pc_mic);
        assert!(s.pc_mic_device.is_empty());
        assert_eq!(s.pc_mic_gain_db, -3.0);
    }

    #[test]
    fn legacy_pc_mic_off_missing_or_malformed_never_enables_a_microphone() {
        for raw in [
            json!({}),
            json!({"voiceMicDevice": "Recorder only"}),
            json!({"phoneDesktopMic": false, "voiceMicDevice": "Recorder only"}),
            json!({"phoneDesktopMic": "true", "voiceMicDevice": "Recorder only"}),
        ] {
            let (s, _) = load(raw);
            assert!(!s.pc_mic);
            assert!(!s.radios[0].pc_mic);
            assert!(!s.unknown.contains_key("phoneDesktopMic"));
        }
    }

    #[test]
    fn legacy_pc_mic_missing_input_stays_unselected_without_a_default_fallback() {
        let (s, _) = load(json!({"phoneDesktopMic": true}));
        assert!(s.pc_mic);
        assert!(s.pc_mic_device.is_empty());
        assert_eq!(s.pc_mic_gain_db, 0.0);
    }
}
