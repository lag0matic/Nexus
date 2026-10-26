//! Local microphone adapter for the same guarded latch as streamed voice. The
//! feeds remain separate: a remote sender cannot supply or release local audio.

use super::Engine;
use crate::mic::{MicEnded, MicFeed, MicFrame, MIC};
use crate::settings::OperatingMode;
use std::time::Instant;

pub(super) struct DesktopMic {
    pub feed: MicFeed,
    device: String,
    seq: u64,
    media: u64,
    armed: Instant,
}

impl Engine {
    pub(super) fn arm_desktop_mic(&mut self, now: Instant) -> bool {
        if self.mic.active() {
            return self.desktop_mic.is_some();
        }
        let device = self.settings.voice_mic_device.trim();
        if !self.settings.phone_desktop_mic
            || device.is_empty()
            || device.eq_ignore_ascii_case(self.settings.audio_in.trim())
            || device.eq_ignore_ascii_case(self.settings.audio_out.trim())
            || self.settings.operating_mode != OperatingMode::Phone
            || !self.tx_enabled
            || !self.tx_allowed()
            || self.tuning
            || self.tx_owner().is_some()
            || self.clock_repair_holds_tx()
            || self.remote_transmit_held()
        {
            return false;
        }
        let feed = MicFeed::default();
        feed.open();
        self.desktop_mic = Some(DesktopMic {
            feed,
            device: device.to_owned(),
            seq: 0,
            media: 0,
            armed: now,
        });
        self.mic.arm();
        self.mic_presence = None;
        self.mic_rf_seen = false;
        self.mic_zero_seen = false;
        self.mic_ended = None;
        self.reset_tx_watchdog();
        true
    }

    /// The service opens only this named input, never the shared receiver tap.
    pub fn desktop_mic_device(&self) -> Option<&str> {
        self.desktop_mic.as_ref().map(|m| m.device.as_str())
    }

    pub(super) fn desktop_mic_current(&self, now: Instant) -> bool {
        self.desktop_mic.as_ref().is_some_and(|m| {
            self.settings.phone_desktop_mic
                && self.settings.voice_mic_device.trim() == m.device
                && (m.media > 0 || now.saturating_duration_since(m.armed).as_millis() < 1_000)
        })
    }

    /// Already-resampled capture, before the common pacer runs. A stalled producer
    /// cannot dump a backlog onto the air, nor reuse audio from an earlier press.
    pub fn push_desktop_mic(&mut self, samples: &[f32], now: Instant) {
        if samples.len() > MIC.samples(MIC.gap_ms) as usize {
            if self.desktop_mic.is_some() {
                self.drop_mic_latch_for(MicEnded::AudioGap);
            }
            return;
        }
        let Some(mic) = self.desktop_mic.as_mut() else {
            return;
        };
        for chunk in samples.chunks(MIC.samples(20) as usize) {
            mic.seq += 1;
            let frame = MicFrame {
                seq: mic.seq,
                media: mic.media,
                arrived: now,
                samples: chunk
                    .iter()
                    .map(|x| {
                        if x.is_finite() {
                            x.clamp(-1.0, 1.0)
                        } else {
                            0.0
                        }
                    })
                    .collect(),
            };
            mic.media += chunk.len() as u64;
            if mic.feed.push(frame).is_err() {
                self.drop_mic_latch_for(MicEnded::AudioGap);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mic::MicTick;
    use std::time::Duration;

    fn scene() -> Engine {
        let mut e = Engine::new("W9XYZ", "EN52", 0);
        e.set_operating_mode("phone", false);
        e.settings.phone_desktop_mic = true;
        e.settings.voice_mic_device = "Desktop microphone".into();
        e
    }

    #[test]
    fn desktop_arms_without_keying_then_uses_only_its_own_feed() {
        let mut e = scene();
        let now = Instant::now();
        assert!(e.arm_desktop_mic(now));
        assert!(!e.manual_ptt());
        assert_eq!(e.poll_mic(now, 40.0, 1), MicTick::Armed);
        assert!(!e.arm_remote_mic(now));
        e.release_remote_mic();
        assert!(e.mic_armed());
        e.push_desktop_mic(&[0.25; 240], now);
        assert_eq!(e.poll_mic(now, 40.0, 1), MicTick::Key);
        e.set_ptt(false);
        assert!(!e.mic_armed());
        assert!(e.take_mic_abort());
        e.push_desktop_mic(&[0.9; 240], now);
        assert!(e.arm_desktop_mic(now));
        assert_eq!(e.poll_mic(now, 40.0, 1), MicTick::Armed);
    }

    #[test]
    fn a_sign_in_hold_refuses_desktop_ptt_under_an_armed_latch() {
        let mut e = scene();
        assert!(e.tx_enabled());
        e.set_remote_transmit_held(true);
        e.set_ptt(true);
        assert!(!e.mic_armed(), "desktop PTT must honor the sign-in hold");
        e.set_ptt(false);
        e.set_remote_transmit_held(false);
        e.set_ptt(true);
        assert!(e.mic_armed(), "the same press arms once the hold is lifted");
        e.set_ptt(false);
        assert!(!e.mic_armed());
    }

    #[test]
    fn no_fallback_to_receiver_missing_mic_or_disabled_tx() {
        let mut e = scene();
        let now = Instant::now();
        e.settings.voice_mic_device.clear();
        assert!(!e.arm_desktop_mic(now));
        e.settings.voice_mic_device = "Radio codec".into();
        e.settings.audio_in = "Radio codec".into();
        assert!(!e.arm_desktop_mic(now));
        e.settings.voice_mic_device = "Desktop microphone".into();
        e.set_tx_enabled(false);
        assert!(!e.arm_desktop_mic(now));
    }

    #[test]
    fn missing_audio_device_changes_and_route_changes_end_the_over() {
        for cause in 0..4 {
            let mut e = scene();
            let now = Instant::now();
            assert!(e.arm_desktop_mic(now));
            e.push_desktop_mic(&[0.25; 240], now);
            assert_eq!(e.poll_mic(now, 40.0, 1), MicTick::Key);
            let route = if cause == 2 { 2 } else { 1 };
            if cause == 0 {
                e.settings.voice_mic_device = "Other microphone".into();
            }
            if cause == 1 {
                e.settings.phone_desktop_mic = false;
            }
            if cause == 3 {
                e.halt_tx();
            }
            assert_eq!(
                e.poll_mic(now + Duration::from_millis(20), 40.0, route),
                MicTick::Idle
            );
            assert!(!e.mic_armed());
            assert!(e.take_mic_abort());
        }
    }

    #[test]
    fn capture_stall_and_backlog_stop_without_rearming() {
        for backlog in [false, true] {
            let mut e = scene();
            let now = Instant::now();
            assert!(e.arm_desktop_mic(now));
            e.push_desktop_mic(&[0.25; 240], now);
            assert_eq!(e.poll_mic(now, 40.0, 1), MicTick::Key);
            if backlog {
                e.push_desktop_mic(&[0.25; 2401], now);
            }
            assert_eq!(
                e.poll_mic(now + Duration::from_millis(250), 40.0, 1),
                MicTick::Idle
            );
            assert!(e.take_mic_abort());
            e.push_desktop_mic(&[0.25; 240], now);
            assert!(!e.mic_armed());
        }
    }
}
