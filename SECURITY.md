# Security policy

## Reporting a vulnerability

Please report security problems privately, not in a public issue: open the repository's **Security** tab and choose **Report a vulnerability**. Include what you found, how to reproduce it, and which version and system you used.

You will get an answer within a week. Once the problem is fixed and a new version is out, the report can be published with credit to you, if you like.

## Supported versions

Security fixes go into the latest release and `main`.

## What to look at

These parts of Singline handle data from outside and deserve the most attention:

- **The browser bridge.** The app listens on `127.0.0.1` (ports 17464 to 17466) for the browser extension. It must refuse web pages, other hosts and requests without the token from the pairing.
- **The Spotify connection.** The login uses PKCE with a short-lived local listener; the refresh token is kept in the system's credential store.
- **Imported files.** LRC files and lyrics from LRCLIB are parsed and shown, never executed.
- **The microphone**, once "Sing to find" is available: audio must stay on the device.

Singline has no server, no account and no telemetry. The only services it talks to are LRCLIB, and Spotify or other services only when you connect them.
