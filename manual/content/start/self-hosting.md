+++
title = "Self-hosting"
description = "Run the web app as one process serving one vault: Docker, configuration, and authentication."
weight = 2
+++

KizunaShelf can run as a self-hosted web app: one process serves the API and the web UI, pointed at a single vault. This guide covers running, configuring, and securing that deployment — every environment variable is in [Configuration](#configuration) below. For the vault schema itself, see the [reference](@/reference/config.md).

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
| `KIZUNASHELF_WEB_DIST` | Alternate web build path. |
| `KIZUNASHELF_SERVE_WEB` | Set to `false` to serve only the API. |
| `KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS` | Set to `true` to let asset downloads reach private/loopback/link-local addresses (e.g. a LAN image host). Off by default; the SSRF guard blocks them (the `198.18.0.0/15` benchmarking range is always allowed). |
| `KIZUNASHELF_TRAKT_CLIENT_ID` | Trakt client id (its `trakt-api-key`) for [importing](@/reference/external.md#quick-capture-and-import) a Trakt profile. |
| `KIZUNASHELF_STEAM_API_KEY` | Steam Web API key for importing a Steam library (`GetOwnedGames`). Distinct from the keyless store API the Steam search provider uses. |

**Search-provider credentials** (IGDB, TheTVDB, TMDB, Discogs, MyAnimeList, BGG, Comic Vine, Hardcover, Google Books, …) are also plain environment variables, listed per provider — with each credential's exact env var — on the generated [External providers](@/reference/providers.md) page. Each is derived mechanically from the credential key a provider (or [import source](@/reference/external.md#quick-capture-and-import)) declares in its catalog (`KIZUNASHELF_<UPPER_KEY>`), so a new credentialed provider needs no change here. Only the web runtime reads these variables — the desktop and iOS apps store credentials in the OS keychain instead.

### Persistent data and caches

The mounted vault is the durable user data: Markdown entities, daily notes, downloaded assets, saved lists, and `KizunaShelf/config.yaml` all live there. Back up or version that folder as you would any other document library.

Two optional host paths stay deliberately outside the vault:

- `KIZUNASHELF_TOKEN_CACHE` stores derived provider OAuth tokens. The default is a temporary file; losing it only forces KizunaShelf to obtain another token.
- `KIZUNASHELF_INDEX_CACHE_DIR` stores a disposable parsed-file index that speeds cold starts for large vaults. It can be placed on a persistent container volume, but it is always safe to delete.

Neither path should be synchronized as part of the vault, and provider credentials should continue to enter the container as secrets or environment variables rather than files inside the vault.

### Health check

`GET /api/health` returns success after the server can load the configured vault. It can be used by a container health check or reverse proxy, but keep it behind the same access boundary as the app: its response includes library counts and a bounded list of diagnostics.

### Multiple vaults

A web instance serves exactly one vault. To host more than one, run multiple instances — each with its own `KIZUNASHELF_VAULT_ROOT` and `PORT` (or its own container with its own vault mount and published port). The image is small.

## Authentication

**KizunaShelf has no built-in authentication for now.** The web server does not implement user accounts, logins, or sessions, so anyone who can reach the port can use it. Two consequences:

- **Do not expose the port directly to the internet.** Keep it bound to loopback (`HOST=127.0.0.1`, the default) or to a private network, and put an authenticating layer in front of it.
- **Writes default off for non-loopback hosts.** When `HOST` is not loopback, content and Settings writes are disabled unless you explicitly opt in (see above). This limits mutation, but read-only access still exposes the contents of the vault and is not a substitute for authentication.

To require a login, run KizunaShelf behind a **reverse proxy that handles authentication** and only forward authenticated requests to the app. Any existing solution works — for example:

- **[Authentik](https://goauthentik.io/)** — full identity provider with forward-auth/proxy support.
- **[Authelia](https://www.authelia.com/)** — authentication and authorization server designed to sit in front of a reverse proxy (Nginx, Traefik, Caddy).
- **[Tinyauth](https://tinyauth.app/)** — a lightweight option when you just want a simple login in front of the app.

A typical setup is: reverse proxy (Nginx / Traefik / Caddy) terminates TLS, delegates authentication to one of the above via forward-auth, and proxies the authenticated request to KizunaShelf on `127.0.0.1:8787`. KizunaShelf itself stays bound to loopback and never sees an unauthenticated request.

## Related pages

- [Configuration overview](@/reference/config.md) — the vault schema: what it is, where it lives, and the map of the reference.
- [Syncing your vault](@/guides/syncing.md) — how to sync a vault across devices.
