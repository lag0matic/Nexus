# Nexus Longwave .11

Browser panels can now start with Nexus. Enable Start browser sharing with
Nexus in the local Browser panels dialog, then pair the phone with Remember
this browser selected. Refreshing, opening another panel, and restarting the
browser no longer require re-entering the code. The laptop's OS credential store
keeps the pairing across Nexus restarts. Automatic startup is off by default.

Stop sharing pauses access while retaining pairing and the startup preference.
Forget paired browsers stops sharing, disables automatic startup and revokes all
saved access. Forget this browser removes access from that browser only. A
revoked browser returns to the code prompt, while temporary network failures
preserve pairing. If credential storage is unavailable, a warning explains that
manual sharing will not survive a Nexus restart.

The .10 panel library and POTA filters are included. Browser actions remain
receive-only: no PTT, arming, transmit, automatic tuning, log editing or ATU.
Sharing uses unencrypted HTTP on a trusted private LAN; do not forward the port
or expose it online. Keep a stable laptop hostname/IP for the phone bookmark.
Automatic startup uses the configured address only, rather than following the
laptop onto another private-address network.
