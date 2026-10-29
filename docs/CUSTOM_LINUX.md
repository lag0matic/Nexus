# Custom Longwave Linux Build

Version: `1.21.0-longwave.9`.
Branch: `codex/upstream-linux-20261029`.
Upstream integrated: `a4df10bf967783dce386f69e60dc98ffb9b060aa`.

This includes upstream's unreleased work, not a new official Nexus release.
The previous custom branch, `codex/lan-observer-panels`, is unchanged.

## Retained Changes

- Longwave logbook selection, history, synchronization and per-contact upload results.
- Desktop microphone routing with release-safe PTT.
- IC-7300 PBT and manual notch controls. Tune switches the internal tuner
  inline/bypass; ATU starts its tuning cycle. Tune does not generate a carrier.
- Receive-only LAN browser panels for POTA, Conditions and Needed, with a short
  address and local eight-digit access code. Selection changes frequency/mode
  without transmitting or arming a digital transmission.
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
Browser access does not encrypt traffic. An intermittent Linux UI-freeze report
from the previous build remains unconfirmed; this merge is not a claimed fix.
