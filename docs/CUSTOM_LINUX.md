# Custom Longwave Linux Build

Version: `1.23.0-longwave.15`.
Branch: `codex/upstream-linux-20261106`.
Upstream integrated: `e3b9ef1e68d9d2305db7c67da6ff34d85d2d796c`.

This includes upstream's unreleased work, not a new official Nexus release.
Upstream 1.23.0's scope layouts, station profiles and logging intake are included.
With Split Operation set to Rig, a digital screen can leave split active with
TX disabled. Switch to Phone/CW before using a rig microphone or foot switch;
see upstream's 1.23.0 release notes for the split and FT2 timing limitations.
The previous custom branches, `codex/lan-observer-panels` and
`codex/upstream-linux-20261029`, are unchanged.

## Retained Changes

- Named PipeWire receive-monitor outputs, separate from the radio's capture/TX
  selectors. Choose a `PipeWire: ...` entry for desktop volume and normal port
  switching. Requires PipeWire 1.0 or newer, a session manager and `pw-dump`.
  The selected sink must resolve to a physical ALSA card different from every
  guarded TX card. Virtual/ambiguous/missing outputs are refused; playback never
  substitutes the default sink. An unavailable built-in output needs attention
  in the desktop audio settings before it can be selected in Nexus.
- Browser station selections fill the Phone/CW contact draft without logging,
  transmitting or arming digital operation. Native and browser spot selections
  preserve the current hardware scope span until the operator changes it.
- Conditions uses the available browser height in desktop-shaped windows.
- Browser POTA hides log-confirmed worked-today activations by default, as the
  native board does. The checkbox reveals those rows with a Worked today label.
  The same activator at another park remains eligible; the station clock rolls
  eligibility at 0000Z. The browser updates on its regular 30-second refresh or
  Refresh panel; selecting a spot alone never hides it or creates a contact.

- Longwave logbook selection, history, synchronization and per-contact upload results.
- Upstream live PC-microphone routing with release-safe PTT. The custom adapter
  is removed. Its enabled state and selected input migrate once to the active
  radio's upstream PC-mic settings; explicit upstream choices are preserved.
  Voice-keyer recording retains its separate microphone setting. Other radios
  are not enabled by the migration. New microphone gain starts at 0 dB.
- IC-7300 PBT and manual notch controls. Tune switches the internal tuner
  inline/bypass; ATU starts its tuning cycle. Tune does not generate a carrier.
- Receive-only LAN browser views for all 18 native pop-out panel types, with a
  short address and local eight-digit access code. The panel picker opens
  independent routes; the external-link button opens another browser tab.
  POTA has band and exact-mode filter buttons. A manual refresh preserves the
  current access and filters. Listen changes frequency/mode without transmitting
  or arming a digital transmission.
- Extra browser views are purpose-built monitors, not copies of native transmit
  cockpits: spots and receive memories support listen; contest/activation/club
  screens show bounded status tables; the station map, audio waterfall and SSTV
  preview/latest picture are view-only. No log editing, arming, PTT, ATU or
  Doppler/automatic tuning is available through these browser panels.
- Stock updater prompts and downloads remain disabled for custom builds.
- Browser pairing survives refresh. With Remember this browser selected, it also
  survives browser and Nexus restarts. Enable Start browser sharing with Nexus
  in the local Browser panels dialog for automatic startup (off by default).
  Stop pauses sharing without forgetting browsers; Forget paired browsers stops
  sharing, disables automatic startup and revokes all saved access. Pairing is
  kept in the OS credential store, separate from Remote. If that store is locked
  or unavailable, manual sharing remains available with a warning, but a new
  Nexus session requires pairing again. No plaintext-file fallback is used.

Longwave uploads and stamps are integrated into upstream's replacement Contact
Card. The Longwave upload column now sorts through the same engine query path
as the other services. Unrecognized upload outcomes stay blocked from retry.

## Building

See `LINUX.md` for system dependencies. The custom x86_64 package is built in
Ubuntu 22.04 with Rust 1.93.1 and Node 24, keeping the normal release profile.

Do not reuse a native modem archive from a different checkout or ABI revision.
When adopting an older build cache, clean `tempo-fast-sys` in both Cargo
workspaces before building: `cargo clean -p tempo-fast-sys` and
`cargo clean --manifest-path src-tauri/Cargo.toml -p tempo-fast-sys`. A stale
Fortran archive can link successfully against newer Rust declarations and then
crash at the FFI boundary. Keep the target directories scoped to the build.

Stage the ignored DeepCW model and metadata according to
`src-tauri/resources/deepcw/README.md`. Do not commit the model files.
Stage the licensed IOTA resource before bundling:

```sh
node scripts/gen-iota-groups.mjs
scripts/build-linux.sh
```

The build produces an AppImage and Debian package under
`src-tauri/target/release/bundle/`. Hamlib is bundled by the Linux build script.
Retain the bundled notices and model metadata when redistributing.

## Verification And First Run

Run `scripts/gates --job test,test-features,test-tauri,ui` and real-browser probes
before packaging. A partial gate result is not a full CI matrix pass. In
particular, Linux checks do not certify Windows, macOS, Android or real RF I/O.

Close the previous Nexus instance and back up the laptop's Nexus settings and
local log before trying this build. Keep the previous package as well as that
data backup. The new build should retain saved Longwave settings and certificate
approval; never work around a new TLS failure by disabling verification.

Use LAN sharing only on a trusted private network, without port forwarding.
Remembered browser access is stored for the exact address and port. A hostname
or DHCP reservation avoids a changed laptop IP invalidating the phone bookmark.
Automatic startup retries for up to a minute while the laptop network reconnects.
It binds only the configured private address, not a newly detected Wi-Fi address.
If DHCP changes that address, reconfigure it locally or use a reservation.

If the desktop microphone keys the IC-7300 but USB audio stops reaching it,
check both the configured playback device and the rig's modulation input.
On 2026-10-30, disconnecting and reconnecting the radio USB cable restored
audio on the laptop without a software change. The microphone patch was not
removed; the observed failure was cleared by re-enumerating the USB device.
Browser access does not encrypt traffic. An intermittent Linux UI-freeze report
from the previous build remains unconfirmed; this merge is not a claimed fix.

## PC Microphone Acceptance

Configure Settings > Radio > Audio > Use this computer's microphone. On Linux,
select distinct named physical cards for microphone and radio receive; upstream
refuses sound-server aliases and ambiguous routing. Keep the IC-7300's DATA OFF
MOD set to USB for ordinary USB/LSB voice. PTT release may play up to 200 ms of
audio already captured; Stop TX, TX Off and faults unkey without that tail.

Source and simulated checks cannot certify the laptop's actual audio devices.
Verify microphone audio, release/Stop, rapid re-press and USB-disconnect behavior
on the IC-7300 before normal operation. Keep the verified .9 package and a
settings/local-log backup for rollback.
