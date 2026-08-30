---
title: "Self-hosting"
description: "Run the web app as one process serving one vault: Docker, configuration, and authentication."
sidebar_position: 2
---

KizunaShelf can run as a self-hosted web app: one process serves the API and the web UI, pointed at a single vault. This guide covers running, configuring, and securing that deployment — every environment variable is in [Configuration](#configuration) below. For the vault schema itself, see the [reference](../reference/config.md).

## Run with Docker

```bash
docker run -p 8787:8787 -v /path/to/vault:/vault ghcr.io/mudkipme/kizunashelf:latest # coming soon
```

Mount the vault directory and the server uses it directly — `KIZUNASHELF_VAULT_ROOT` defaults to `/vault`. There is no app config file to mount. One process serves both the UI and the API on the same origin, listening on port `8787` (the server enables no wildcard CORS — don't host the static assets separately).

The Docker image sets `HOST=0.0.0.0` by default, so the server is reachable from outside the container. Because that is a non-loopback host, content and Settings writes default to **off**. Set `KIZUNASHELF_CONTENT_WRITABLE=true` and/or `KIZUNASHELF_SETTINGS_WRITABLE=true` only when you intentionally want writes available from the published container.

For an enforced read-only deployment, mount the vault read-only as well as leaving both write flags disabled:

```bash
docker run -p 8787:8787 -v /path/to/vault:/vault:ro ghcr.io/mudkipme/kizunashelf:latest
```

For a writable deployment, the vault mount itself must be writable and the container user must have permission to create, rename, and move files inside it. KizunaShelf uses atomic writes and moves deleted content into the vault's `.trash` directory.

## Configuration

The self-hosted web server is configured entirely through environment variables — there is no app config file:

| Variable | Description |
| --- | --- |
| `KIZUNASHELF_VAULT_ROOT` | Absolute path to the single vault this instance serves. Defaults to `/vault` (the conventional Docker mount). |
| `HOST` | Bind host. Defaults to `127.0.0.1`. Set `HOST=0.0.0.0` only when you intentionally want to expose it beyond the local machine. |
| `PORT` | Bind port. Defaults to `8787`. |
| `KIZUNASHELF_CONTENT_WRITABLE` | Enables entity create/edit/delete. Defaults to `true` for loopback hosts and `false` otherwise. |
| `KIZUNASHELF_SETTINGS_WRITABLE` | Enables schema (Settings) writes and path suggestions. Defaults to `true` for loopback hosts and `false` otherwise. |
| `KIZUNASHELF_CACHE_TTL_MS` | In-memory library cache TTL. Defaults to `10000`. |
| `KIZUNASHELF_INDEX_CACHE_DIR` | Optional host directory for the persistent, disposable library index cache. Keep it outside the vault; unset means the incremental index lives only in memory and is lost on restart. |
| `KIZUNASHELF_TOKEN_CACHE` | Path for the provider OAuth token cache. Defaults to `<tmp>/.kizunashelf.tokens.json` (outside the vault). |
| `RUST_LOG` | Log verbosity. Defaults to `info`. Set `RUST_LOG=kizunashelf=debug` for per-request access logs and cache decisions — see [Logs](#logs). |
| `KIZUNASHELF_WEB_DIST` | Alternate web build path. |
| `KIZUNASHELF_SERVE_WEB` | Set to `false` to serve only the API. |
| `KIZUNASHELF_AUTH_PASSWORD_HASH` | Optional Argon2id password hash. When set, all web UI, API, and asset requests require a login. Unset by default. |
| `KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS` | Set to `true` to let asset downloads reach private/loopback/link-local addresses (e.g. a LAN image host). Off by default; the SSRF guard blocks them (the `198.18.0.0/15` benchmarking range is always allowed). |
| `KIZUNASHELF_TRAKT_CLIENT_ID` | Trakt client id (its `trakt-api-key`) for [importing](../reference/external.md#quick-capture-and-import) a Trakt profile. |
| `KIZUNASHELF_STEAM_API_KEY` | Steam Web API key for importing a Steam library (`GetOwnedGames`). Distinct from the keyless store API the Steam search provider uses. |

**Search-provider credentials** (IGDB, TheTVDB, TMDB, Discogs, MyAnimeList, BGG, Comic Vine, Hardcover, Google Books, …) are also plain environment variables, listed per provider — with each credential's exact env var — on the generated [External providers](../reference/providers.md) page. Each is derived mechanically from the credential key a provider (or [import source](../reference/external.md#quick-capture-and-import)) declares in its catalog (`KIZUNASHELF_<UPPER_KEY>`), so a new credentialed provider needs no change here. Only the web runtime reads these variables — the desktop and iOS apps store credentials in the OS keychain instead.

### Persistent data and caches

The mounted vault is the durable user data: Markdown entities, daily notes, downloaded assets, saved lists, and `KizunaShelf/config.yaml` all live there. Back up or version that folder as you would any other document library.

Two optional host paths stay deliberately outside the vault:

- `KIZUNASHELF_TOKEN_CACHE` stores derived provider OAuth tokens. The default is a temporary file; losing it only forces KizunaShelf to obtain another token.
- `KIZUNASHELF_INDEX_CACHE_DIR` stores a disposable parsed-file index that speeds cold starts for large vaults. It can be placed on a persistent container volume, but it is always safe to delete.

Neither path should be synchronized as part of the vault, and provider credentials should continue to enter the container as secrets or environment variables rather than files inside the vault.

### Logs

The server writes plain-text logs to stdout, so `docker logs` (or your compose/systemd unit's journal) is where they land. Colour codes are emitted only when stdout is a terminal, so captured logs stay clean.

At the default `info` level it stays quiet, reporting only what an operator needs to see:

- every library load from the vault, with how long it took and how many entities, relations, and diagnostics it produced — the number to watch if cold starts feel slow;
- every failed request, with the reason the client was given and the method and path that produced it;
- requests rejected before reaching a handler (a malformed query, a method a route does not accept), which indicate a client/server mismatch.

Routine misses — a request for an entity or asset that does not exist — are not warnings and stay at `debug`, along with an access log line per request and the library cache's reuse decisions. Turn those on with `RUST_LOG=kizunashelf=debug`:

```
INFO  request{method=GET path=/api/health}: reloaded the library from the vault entities=2 relations=0 diagnostics=0 elapsed_ms=26
WARN  request{method=POST path=/api/entities}: request rejected status=409 Entity file already exists
DEBUG request{method=GET path=/api/health}: vault listing unchanged; reusing the cached library
```

Nothing is written into the vault, and no log file is created or rotated by KizunaShelf itself — collecting and retaining the output is your container runtime's job.

### Health check

`GET /api/health` returns success after the server can load the configured vault. It can be used by a container health check or reverse proxy, but keep it behind the same access boundary as the app: its response includes library counts and a bounded list of diagnostics.

### Multiple vaults

A web instance serves exactly one vault. To host more than one, run multiple instances — each with its own `KIZUNASHELF_VAULT_ROOT` and `PORT` (or its own container with its own vault mount and published port). The image is small.

## Authentication

KizunaShelf provides optional single-user password authentication for the web runtime. Desktop and iOS do not use it. Authentication is disabled when `KIZUNASHELF_AUTH_PASSWORD_HASH` is unset.

### Generate the password hash

Generate an Argon2id hash interactively. The command prompts twice without echoing the password, then prints the hash:

```bash
cargo run -p kizunashelf --bin kizunashelf-api -- hash-password
```

With the published container image, run the same helper without mounting a vault:

```bash
docker run --rm -it ghcr.io/mudkipme/kizunashelf:latest kizunashelf-api hash-password
```

Pass the printed value to the server. Keep the single quotes: Argon2 hashes contain `$` characters that the shell would otherwise expand.

```bash
docker run -p 8787:8787 \
  -v /path/to/vault:/vault \
  -e 'KIZUNASHELF_AUTH_PASSWORD_HASH=$argon2id$v=19$m=19456,t=2,p=1$…' \
  ghcr.io/mudkipme/kizunashelf:latest
```

Successful login creates a secure, HTTP-only, same-site cookie. Sessions last at most 30 days and are held only in server memory, so restarting KizunaShelf signs every browser out. To end the current session manually, open `/_auth/logout` on your KizunaShelf host.

### Login throttling and your proxy

Failed logins are limited to five per minute **per client**, after which that client gets `429 Too Many Requests` until the minute is up. Other clients are unaffected — a single-password deployment has exactly one legitimate user, and a shared counter would let anyone who can reach the login form lock that user out by guessing badly on purpose.

Telling clients apart needs the real client address, and behind a reverse proxy every request arrives from the proxy. So when the connection comes from **loopback, a private network (RFC 1918 / ULA), a link-local address, or the carrier-grade-NAT range Tailscale uses**, KizunaShelf reads the client from `X-Forwarded-For` (falling back to `X-Real-IP`), walking past any further hops that are themselves inside that boundary. A request arriving straight from a public address is charged to that address whatever its headers claim, so the headers cannot be used to shed identity and out-run the throttle.

**What this means for your proxy config:** if it does not set `X-Forwarded-For` or `X-Real-IP`, every visitor shares one bucket and the limit is effectively global again. Nginx needs `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;`; Caddy and Traefik set it by default. IPv6 clients are counted per `/64`, since one machine is routinely handed that whole range.

### Deployment boundary

- **Continue to use HTTPS.** The session cookie is deliberately marked `Secure`; terminate TLS at a reverse proxy and forward to KizunaShelf over a private or loopback connection.
- **Do not expose the application port directly to the internet.** Keep it bound to loopback (`HOST=127.0.0.1`, the default) or to a private container network. Authentication limits application access but does not provide TLS or network hardening.
- **Writes still default off for non-loopback hosts.** Set the write flags explicitly when the authenticated deployment should allow changes.
- The existing diagnostic-rich `/api/health` endpoint is authenticated too. When built-in authentication is enabled, an unauthenticated `GET /healthz` returns an empty `204 No Content` response for container and proxy health checks without exposing vault diagnostics.

For deployments that already have centralized identity, leave `KIZUNASHELF_AUTH_PASSWORD_HASH` unset and put KizunaShelf behind a reverse proxy that handles authentication. Any existing forward-auth solution works—for example:

- **[Authentik](https://goauthentik.io/)** — full identity provider with forward-auth/proxy support.
- **[Authelia](https://www.authelia.com/)** — authentication and authorization server designed to sit in front of a reverse proxy (Nginx, Traefik, Caddy).
- **[Tinyauth](https://tinyauth.app/)** — a lightweight option when you just want a simple login in front of the app.

A typical setup is: reverse proxy (Nginx / Traefik / Caddy) terminates TLS, delegates authentication to one of the above via forward-auth, and proxies the authenticated request to KizunaShelf on `127.0.0.1:8787`.

## Related pages

- [Configuration overview](../reference/config.md) — the vault schema: what it is, where it lives, and the map of the reference.
- [Syncing your vault](../guides/syncing.md) — how to sync a vault across devices.
