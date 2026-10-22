# Security

Bomberust is a game for friends on a network you trust. Read this page before you let anybody
else reach it.

## What is not protected

- **No encryption.** Everything travels in clear text: the page, the WebSocket and the terminal
  protocol. Anybody on the path can read it.
- **No authentication.** Anybody who can reach the ports can join and play. Names are only
  labels; two players can have the same name.
- **No rate limiting** beyond the limits below, and **no isolation between spectators and
  players**: every participant receives the public view of the match.

## Safe defaults

| Setting | Default | Why |
|---------|---------|-----|
| `bomberust-server --bind` | `127.0.0.1` | only this machine can connect |
| Web page for `bomberust-cli host --web-port` | `127.0.0.1` | the page is only offered locally unless you pass `--bind` |
| WebSocket `Origin` | same origin only | a web page from another site cannot drive your game |

The game port of `bomberust-cli host` listens on every interface (`0.0.0.0`), as it always did,
so friends can join by address. The program prints a `WARNING:` line to stderr when it listens
on anything but loopback. Use `--bind 127.0.0.1` to play only on this machine.

## Exposing the game to a network

Passing `--bind 0.0.0.0` (or a LAN address) makes the web page, the WebSocket and the terminal
port reachable by every machine that can route to you. `bomberust-server` prints:

```text
WARNING: listening on 0.0.0.0: the game has no encryption and no authentication, ...
```

Do this only on a network you trust, or put one of these in front:

- a **firewall** rule that allows only your friends' addresses;
- an **SSH tunnel**: `ssh -L 8080:127.0.0.1:8080 -L 7878:127.0.0.1:7878 you@server`, with the
  server bound to loopback, so the traffic is encrypted and authenticated by SSH;
- a **reverse proxy with TLS** (for example nginx or Caddy) that terminates HTTPS and forwards
  `/` and the `/ws` WebSocket to the loopback web port. Pass the public origin with
  `--allow-origin https://play.example.org` only if the page is served from a different origin
  than the WebSocket; behind a proxy that keeps the `Host` header the default works.

## The Origin and CORS policy

Browsers let any web page open a WebSocket to any address, including `127.0.0.1`. Without a
check, a page on a hostile site could connect to a game running on your machine and drive it.
The WebSocket handshake therefore carries an `Origin` header and the server decides:

1. `Origin` equal to the server's own (`http://` plus the request's `Host`): accepted.
2. `Origin` listed with `--allow-origin <ORIGIN>` (repeatable): accepted, and a CORS warning is
   printed at start-up. Static responses also get `Access-Control-Allow-Origin`.
3. `--allow-any-origin`: every origin is accepted. A loud `WARNING:` is printed; do not use it on
   a shared network.
4. Anything else gets `403 Forbidden` and nothing reaches the match.
5. A handshake **without** `Origin` (command-line tools) is accepted only from a loopback peer.

This is what you need if you open the page from another origin, for example a static copy of the
page on another port: start the server with `--allow-origin http://localhost:3000`.

## Limits

| Limit | Value |
|-------|-------|
| HTTP request head | 8 KiB (`431`) |
| Time to send a request | 5 s |
| Open HTTP and WebSocket connections | 64 by default (`--max-connections`), then `503` |
| Methods | `GET` and `HEAD` only (`405`) |
| Message size | 64 KiB |
| Malformed messages | 10, then the connection is closed |
| Silence during a match | 10 s, then a bot takes over |

The server serves only its six embedded files on seven fixed routes; it never reads a path from the request, so there
is no directory traversal. The page runs with a strict Content Security Policy
(`default-src 'self'`, no inline scripts) and sets `X-Content-Type-Options: nosniff` and
`Referrer-Policy: no-referrer`.

## Reporting a problem

This is a learning project. If you find a hole, open an issue or send a patch with a test next to
the ones in `crates/web/tests/origin.rs` and `crates/web/tests/http.rs`.
