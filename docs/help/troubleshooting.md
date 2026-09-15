# Troubleshooting
> **Job:** Fix the common failures.

## Computer actions do nothing

You need an **X11** session: `echo $XDG_SESSION_TYPE` must print `x11`. On Wayland, `xdotool`/`wmctrl` cannot move the mouse — log into an Xorg session. Then install `xdotool imagemagick wmctrl`.

## Empty accessibility tree

`computer.observe` reports no elements when the AT-SPI bus is unreachable. On stock GNOME it runs by default; minimal window managers need `at-spi2-registryd` started with the session.

## Keyring errors headless

Secrets live in the OS keyring ( GNOME Keyring / KWallet / macOS Keychain). Over SSH without D-Bus, set env vars instead (see [Environment](../reference/environment.md)).

## OAuth loopback blocked

Google/Spotify open a `127.0.0.1` callback listener. Corporate firewalls rarely block loopback, butVPNs with full-tunnel forced routing sometimes do — disconnect the VPN for the 30 seconds of authorization.

## Device flow expired

GitHub/Outlook codes expire after ~10 minutes. Click Connect again for a fresh code.

## Notion "not found"

Share the page or database with the integration (page menu → Connections). Unshared content is invisible to the API by design.

## Discord empty messages

Flip on the **Message Content intent** in the bot portal, then reconnect.

## A connector loops or misbehaves

Every event lands in `connector_logs` in `argus.db` — query it to see exactly which calls failed and why.
