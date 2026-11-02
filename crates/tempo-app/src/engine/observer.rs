//! Receive-only selection from the LAN observer panels. No QSO action or transmit permit.

use super::Engine;
use crate::dto::{Tier, WorkPark};

impl Engine {
    /// Apply one fresh observer click atomically under the caller's engine lock.
    /// Busy requests are refused, never retained. Mode entry cannot arm, and both
    /// sides of the change retire queued transmissions and the Enable TX latch.
    pub fn observer_listen(
        &mut self,
        radio: u32,
        mhz: f64,
        mode: &str,
        call: Option<&str>,
        park: Option<WorkPark>,
    ) -> Result<(), String> {
        let call = call.map(str::trim).filter(|c| !c.is_empty());
        if call.is_some_and(|c| {
            c.len() > 64 || !c.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'/')
        }) || park.as_ref().is_some_and(|p| {
            !matches!(p.program.as_str(), "POTA" | "SOTA")
                || p.reference.is_empty()
                || p.reference.len() > 64
                || !p
                    .reference
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-'))
        }) {
            return Err("Invalid station or park reference".into());
        }
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
        if section == "phone" {
            self.route_intent = Some(if mode.trim().eq_ignore_ascii_case("FM") && mhz >= 29.0 {
                crate::settings::RouteMode::Fm
            } else {
                crate::settings::RouteMode::Ssb
            });
        }
        self.set_frequency(mhz - offset, band, "USB");
        self.sideband_override = match mode.trim().to_ascii_uppercase().as_str() {
            "SSB" => Some(
                if crate::bandplan::phone_is_lsb(mhz) {
                    "LSB"
                } else {
                    "USB"
                }
                .into(),
            ),
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
        // Log-draft metadata only: do not publish a TCI spot click, start a hunt or select a QSO.
        self.work_call = if matches!(section, "phone" | "cw") {
            call.map(str::to_ascii_uppercase)
        } else {
            None
        };
        self.note_work_park(park);
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
    fn observer_spot_metadata_only_fills_phone_and_cw_drafts_without_arming() {
        for mode in ["SSB", "CW", "FT8", "FT4"] {
            let mut e = connected();
            let radio = e.settings().active_radio;
            let park = WorkPark {
                program: "POTA".into(),
                reference: "US-1234".into(),
            };
            e.observer_listen(radio, 14.288, mode, Some(" w1aw/p "), Some(park.clone()))
                .unwrap();
            let manual = matches!(mode, "SSB" | "CW");
            assert_eq!(e.work_call.as_deref(), manual.then_some("W1AW/P"));
            assert_eq!(e.work_park, manual.then_some(park.clone()));
            assert!(e.work_afar);
            assert!(!e.tx_enabled());
            assert!(e.tx_owner().is_none());
            for slot in 1..5 {
                assert!(e.poll_tx(slot).is_empty());
            }
            let tick = e.work_tick;
            e.observer_listen(radio, 14.288, mode, Some("W1AW/P"), Some(park))
                .unwrap();
            assert_eq!(
                e.work_tick,
                tick + 1,
                "the same station can be selected again"
            );
            e.observer_listen(radio, 14.288, mode, None, None).unwrap();
            assert!(e.work_call.is_none());
            assert!(
                e.work_park.is_none(),
                "a bare frequency never reuses the last station's park"
            );
        }
    }

    #[test]
    fn observer_rejects_invalid_draft_hints_before_changing_the_radio() {
        let mut e = connected();
        let radio = e.settings().active_radio;
        let before = e.settings().dial_mhz;
        for (call, park) in [
            (Some("W1AW\nPTT"), None),
            (
                Some("W1AW"),
                Some(WorkPark {
                    program: "TX".into(),
                    reference: "US-1234".into(),
                }),
            ),
        ] {
            assert!(e.observer_listen(radio, 14.288, "SSB", call, park).is_err());
            assert_eq!(e.settings().dial_mhz, before);
            assert_eq!(e.work_tick, 0);
        }
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
            e.observer_listen(radio, 14.074, mode, None, None).unwrap();
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
            assert!(e.observer_listen(id, freq, mode, None, None).is_err());
            assert_eq!(e.settings().dial_mhz, dial);
        }
        e.rig_keyed = true;
        assert!(e.observer_listen(radio, 14.074, "FT8", None, None).is_err());
        e.rig_keyed = false;
        e.set_cat_status(Some(false), String::new());
        assert!(e.observer_listen(radio, 14.074, "FT8", None, None).is_err());
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
        e.observer_listen(radio, 14.08, "FT4", None, None).unwrap();
        assert!(!e.tx_enabled());
        for slot in 1..8 {
            assert!(e.poll_tx(slot).is_empty());
        }
        assert!(e.work_afar);
    }

    #[test]
    fn observer_ssb_selection_does_not_inherit_a_previous_fm_preference() {
        for (mhz, expected) in [(7.18, "LSB"), (14.28, "USB"), (50.16, "USB")] {
            let mut e = connected();
            e.settings.phone_mode = "fm".into();
            e.set_operating_mode("phone", false);
            let radio = e.settings().active_radio;
            e.observer_listen(radio, mhz, "SSB", None, None).unwrap();
            assert_eq!(e.rig_mode_effective(), expected);
            assert!(!e.tx_enabled());
        }
    }
}
