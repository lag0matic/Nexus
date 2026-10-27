//! Receive-only selection from the LAN observer panels. No QSO action or transmit permit.

use super::Engine;
use crate::dto::Tier;

impl Engine {
    /// Apply one fresh observer click atomically under the caller's engine lock.
    /// Busy requests are refused, never retained. Mode entry cannot arm, and both
    /// sides of the change retire queued transmissions and the Enable TX latch.
    pub fn observer_listen(&mut self, radio: u32, mhz: f64, mode: &str) -> Result<(), String> {
        if radio != self.settings.active_radio {
            return Err("The active radio changed; refresh before tuning".into());
        }
        if self.cat_status.0 != Some(true) {
            return Err("The radio is not connected".into());
        }
        if self.rig_keyed || self.tx_owner().is_some() || self.sat_dial_owner.is_some() {
            return Err("The radio is busy; tune from the laptop after it stops".into());
        }
        let band = crate::bandplan::band_for_dial(mhz)
            .filter(|_| mhz.is_finite())
            .ok_or("The spot is not on a supported amateur band")?;
        let (section, tier) = match mode.trim().to_ascii_uppercase().as_str() {
            "CW" => ("cw", None),
            "SSB" | "USB" | "LSB" | "AM" | "FM" => ("phone", None),
            "RTTY" => ("rtty", None),
            "PSK31" | "BPSK31" => ("keyboard", None),
            "FT8" => ("digital", Some(Tier::Ft8)),
            "FT4" => ("digital", Some(Tier::Ft4)),
            "FT2" => ("digital", Some(Tier::Ft2)),
            "JS8" | "JS8CALL" => ("digital", Some(Tier::Js8)),
            "WSPR" => ("digital", Some(Tier::Wspr)),
            "FST4" => ("digital", Some(Tier::Fst4)),
            "FST4W" => ("digital", Some(Tier::Fst4w)),
            "Q65" => ("digital", Some(Tier::Q65)),
            "JT65" => ("digital", Some(Tier::Jt65)),
            "MSK144" => ("digital", Some(Tier::Msk144)),
            _ => return Err("The spot does not identify a supported exact mode".into()),
        };
        self.halt_tx();
        if let Some(tier) = tier {
            self.set_tier(tier);
        }
        self.set_operating_mode_with_arming(section, false, false);
        if section == "keyboard" {
            self.set_psk_mode(tempo_core::psk::PskModeKind::Bpsk31, false)?;
        }
        let offset = self.soundcard_cw_pitch_offset_mhz(mhz);
        self.sideband_override = None;
        if mode.trim().eq_ignore_ascii_case("FM") && mhz >= 29.0 {
            self.route_intent = Some(crate::settings::RouteMode::Fm);
        }
        self.set_frequency(mhz - offset, band, "USB");
        self.sideband_override = match mode.trim().to_ascii_uppercase().as_str() {
            "USB" => Some("USB".into()),
            "LSB" => Some("LSB".into()),
            "FM" => Some("FM".into()),
            "AM" => Some("AM".into()),
            _ => None,
        };
        // Existing cross-window navigation, explicitly marked as non-arming.
        self.work_tick += 1;
        self.work_view = Some(
            if tier == Some(Tier::Js8) {
                "js8"
            } else {
                section
            }
            .into(),
        );
        self.work_afar = true;
        self.work_call = None;
        self.work_park = None;
        self.halt_tx();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected() -> Engine {
        let mut e = Engine::new("W9XYZ", "EN37", 0);
        e.set_license_class("extra");
        e.set_cat_status(Some(true), String::new());
        e
    }

    #[test]
    fn observer_click_switches_modes_but_never_arms_or_schedules_transmit() {
        for (mode, section, tier) in [
            ("CW", "cw", None),
            ("SSB", "phone", None),
            ("FT8", "digital", Some(Tier::Ft8)),
            ("FT4", "digital", Some(Tier::Ft4)),
            ("RTTY", "rtty", None),
            ("PSK31", "keyboard", None),
            ("WSPR", "digital", Some(Tier::Wspr)),
        ] {
            let mut e = connected();
            e.set_tx_enabled(true);
            let radio = e.settings().active_radio;
            e.observer_listen(radio, 14.074, mode).unwrap();
            assert!(!e.tx_enabled(), "{mode} must leave TX disabled");
            assert!(e.tx_owner().is_none());
            assert_eq!(e.work_view.as_deref(), Some(section));
            assert_eq!(
                e.settings().operating_mode,
                match section {
                    "cw" => crate::settings::OperatingMode::Cw,
                    "phone" => crate::settings::OperatingMode::Phone,
                    "rtty" => crate::settings::OperatingMode::Rtty,
                    "keyboard" => crate::settings::OperatingMode::Keyboard,
                    _ => crate::settings::OperatingMode::Digital,
                }
            );
            assert_eq!(e.settings().dial_mhz, 14.074);
            assert!(e.work_afar);
            if let Some(tier) = tier {
                assert_eq!(e.tier(), tier);
            }
            for slot in 1..5 {
                assert!(e.poll_tx(slot).is_empty(), "{mode} emitted on slot {slot}");
            }
            assert!(!e.take_atu_tune());
        }
    }

    #[test]
    fn observer_click_refuses_busy_stale_disconnected_or_invalid_requests_without_mutation() {
        let mut e = connected();
        let radio = e.settings().active_radio;
        let dial = e.settings().dial_mhz;
        for (id, freq, mode) in [
            (radio + 1, 14.074, "FT8"),
            (radio, f64::NAN, "CW"),
            (radio, 1_000_000.0, "CW"),
            (radio, 14.074, "DIGITAL"),
        ] {
            assert!(e.observer_listen(id, freq, mode).is_err());
            assert_eq!(e.settings().dial_mhz, dial);
        }
        e.rig_keyed = true;
        assert!(e.observer_listen(radio, 14.074, "FT8").is_err());
        e.rig_keyed = false;
        e.set_cat_status(Some(false), String::new());
        assert!(e.observer_listen(radio, 14.074, "FT8").is_err());
        assert_eq!(e.settings().dial_mhz, dial);
    }

    #[test]
    fn observer_click_retires_an_existing_ft_cq_run_and_keeps_later_slots_silent() {
        let mut e = connected();
        e.set_tier(Tier::Ft8);
        e.set_frequency(14.074, "20m", "USB");
        e.call_cq(None).unwrap();
        assert!(e.tx_enabled(), "positive control: CQ really was armed");
        let radio = e.settings().active_radio;
        e.observer_listen(radio, 14.08, "FT4").unwrap();
        assert!(!e.tx_enabled());
        for slot in 1..8 {
            assert!(e.poll_tx(slot).is_empty());
        }
        assert!(e.work_afar);
    }
}
