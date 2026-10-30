# Custom Longwave Linux Build

Version: `1.21.0-longwave.10`.
Branch: `codex/lan-panel-library`.
Upstream integrated: `a4df10bf967783dce386f69e60dc98ffb9b060aa`.

This includes upstream's unreleased work, not a new official Nexus release.
The previous custom branches, `codex/lan-observer-panels` and
`codex/upstream-linux-20261029`, are unchanged.

## Retained Changes

- Longwave logbook selection, history, synchronization and per-contact upload results.
- Desktop microphone routing with release-safe PTT.
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

Longwave uploads and stamps are integrated into upstream's replacement Contact
Card. The Longwave upload column now sorts through the same engine query path
as the other services. Unrecognized upload outcomes stay blocked from retry.

## Building

See `LINUX.md` for system dependencies. The custom x86_64 package is built in
Ubuntu 22.04 with Rust 1.93.1 and Node 24, keeping the normal release profile.

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

If the desktop microphone keys the IC-7300 but USB audio stops reaching it,
check both the configured playback device and the rig's modulation input.
On 2026-10-30, disconnecting and reconnecting the radio USB cable restored
audio on the laptop without a software change. The microphone patch was not
removed; the observed failure was cleared by re-enumerating the USB device.
Browser access does not encrypt traffic. An intermittent Linux UI-freeze report
from the previous build remains unconfirmed; this merge is not a claimed fix.
