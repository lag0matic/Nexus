# Nexus Longwave .12

This custom Linux update integrates upstream through 7ed3f41c9 and replaces our
desktop-microphone adapter with upstream's live PC-mic implementation. Configure
it in Settings > Radio > Audio. It has per-radio microphone selection and gain,
feedback checks, device-loss handling, and a release tail bounded to 200 ms.
Stop TX, TX Off and faults do not wait for that tail.

An existing custom desktop-mic selection migrates to the active radio only,
without changing explicit upstream settings or the voice-keyer's recording
input. Other radios are not enabled. The obsolete switch is removed on save.
On Linux use distinct named physical audio cards, not default/pulse/pipewire.
For IC-7300 USB/LSB voice through its USB codec, retain DATA OFF MOD = USB.

Longwave logbook integration, IC-7300 PBT/notch/tuner controls, and all browser
panels are retained. Browser pairing can survive refresh and restarts; automatic
sharing is opt-in and binds only the configured private address. Browser actions
remain receive-only, with no PTT, arming or automatic tuning. HTTP sharing is for
a trusted LAN only, not internet exposure.

Back up settings and the local log and retain the previous working package.
The new microphone path requires laptop/IC-7300 acceptance testing before normal
operation; no on-air success is inferred from automated tests.
