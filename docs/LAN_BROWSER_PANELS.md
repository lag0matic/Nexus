# LAN Browser Panels

The custom Longwave build can show POTA, Conditions and Needed in browsers on the
same trusted network as the laptop. These are second-monitor panels, not remote
operating consoles. Audio, logging and all transmit controls stay on the laptop.

## Sharing

1. Open POTA, Conditions or Needed in Nexus.
2. Click the monitor/phone icon beside its pop-out control.
3. Check the laptop's private IPv4 address and port, then choose **Start sharing**.
4. Open the supplied access link on the other device. The panel tabs open the
   other shared views; separate access links can be opened in multiple browsers.

Sharing is off at every Nexus launch. **Stop sharing** revokes all current links,
not just the panel whose sharing control is open. Closing the sharing dialog or
switching the laptop's operating screen does not stop the server.

The first version uses numeric IPv4 addresses. The default port is TCP 7375. If
the laptop's firewall blocks access, allow that port only on its trusted local
network. Do not forward it through a router or expose it to the Internet.

## Receive-Only Selection

POTA and Needed rows select frequency and exact mode. The Conditions map uses
its existing double-click gesture for a spot. The laptop changes operating
section and digital tier as needed, without starting a QSO or tagging a log.

An accepted selection disables Enable TX and clears queued transmissions. It
does not generate a carrier, operate the ATU, call CQ, answer a station, arm FT
sequencing or offer PTT. Resume transmitting only by an explicit action at the
laptop.

A busy or disconnected radio, an expired click, a changed active radio, or an
unsupported mode is refused. Requests are never kept for later or automatically
retried. A generic Digital label is not enough to guess FT8, and a Needed row
without an exact frequency is not silently sent to a band's default frequency.

## Access and Limits

- This is unencrypted HTTP for a trusted LAN. A hostile network peer could
  intercept traffic. Use a private home-LAN interface, not a public or VPN one.
- Each sharing session has a fresh random access link. Its secret is carried in
  the URL fragment, removed from the visible address on load, and retained only
  in that tab's memory. Reloading requires reopening the access link.
- Do not post access links publicly. Anyone holding a current link can read the
  shared panel data and request receive-only selections until sharing stops.
- The server binds one private IPv4 address, not all interfaces. Data requests
  require the access secret; selections additionally require the matching Origin
  and Host, a fresh radio observation, strict JSON fields and an idle engine.
- Only explicitly listed reads and the dedicated receive-only selection route
  exist. There is no generic desktop-command bridge, log upload or settings API.
- Feeds are cached across viewers. Connection, blocking-work, header, body and
  request-time limits bound the cost of slow or excessive clients.

Custom Longwave builds also skip stock Nexus update checks and downloads, so a
stock-update popup cannot replace these integrations. Install custom builds
manually.
