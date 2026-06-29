# Self-Hosting KizunaShelf (Web)

KizunaShelf can run as a self-hosted web app: one Rust process serves the API and the built web UI, pointed at a single vault. This guide covers building, running, configuring, and securing that deployment. For the full schema and the complete list of environment variables, see [docs/config.md](config.md); for the env-var table specifically, see [Runtime Environment](config.md#runtime-environment).

## Build and serve

```bash
pnpm install
pnpm build
pnpm serve
```

`pnpm serve` runs the Rust API and serves the built Vite app from one process. It listens on `127.0.0.1:8787` by default and reads its single vault from `KIZUNASHELF_VAULT_ROOT`.

The server does not enable wildcard CORS by default. Serve the built web app from the Rust process (as `pnpm serve` does) rather than hosting the static assets separately — that keeps the UI and API on the same origin.

## Docker

Build and run the image from the repository root:

```bash
docker build -t kizunashelf .
docker run -p 8787:8787 -v /path/to/vault:/vault kizunashelf
```

Mount the vault directory and the server uses it directly — `KIZUNASHELF_VAULT_ROOT` defaults to `/vault`. There is no app config file to mount.

The Docker image sets `HOST=0.0.0.0` by default, so the server is reachable from outside the container. Because that is a non-loopback host, content and Settings writes default to **off**. Set `KIZUNASHELF_CONTENT_WRITABLE=true` and/or `KIZUNASHELF_SETTINGS_WRITABLE=true` only when you intentionally want writes available from the published container.

## Configuration

The self-hosted web server is configured entirely through environment variables — there is no app config file. The most relevant ones:

| Variable | Description |
| --- | --- |
| `KIZUNASHELF_VAULT_ROOT` | Absolute path to the single vault this instance serves. Defaults to `/vault` (the conventional Docker mount). |
| `HOST` | Bind host. Defaults to `127.0.0.1`. Set `HOST=0.0.0.0` only when you intentionally want to expose it beyond the local machine. |
| `PORT` | Bind port. Defaults to `8787`. |
| `KIZUNASHELF_CONTENT_WRITABLE` | Enables entity create/edit/delete. Defaults to `true` for loopback hosts and `false` otherwise. |
| `KIZUNASHELF_SETTINGS_WRITABLE` | Enables schema (Settings) writes and path suggestions. Defaults to `true` for loopback hosts and `false` otherwise. |

See [Runtime Environment](config.md#runtime-environment) for the complete table, including the cache TTL, token cache path, asset-download host policy, and the per-provider credential variables.

### Multiple vaults

A web instance serves exactly one vault. To host more than one, run multiple instances — each with its own `KIZUNASHELF_VAULT_ROOT` and `PORT` (or its own container with its own vault mount and published port). The image is small.

## Authentication

**KizunaShelf has no built-in authentication for now.** The web server does not implement user accounts, logins, or sessions, so anyone who can reach the port can use it. Two consequences:

- **Do not expose the port directly to the internet.** Keep it bound to loopback (`HOST=127.0.0.1`, the default) or to a private network, and put an authenticating layer in front of it.
- **Writes default off for non-loopback hosts.** When `HOST` is not loopback, content and Settings writes are disabled unless you explicitly opt in (see above). This limits a fully unauthenticated exposure to read-only browsing, but it is not a substitute for authentication.

To require a login, run KizunaShelf behind a **reverse proxy that handles authentication** and only forward authenticated requests to the app. Any existing solution works — for example:

- **[Authentik](https://goauthentik.io/)** — full identity provider with forward-auth/proxy support.
- **[Authelia](https://www.authelia.com/)** — authentication and authorization server designed to sit in front of a reverse proxy (Nginx, Traefik, Caddy).
- **[Tinyauth](https://tinyauth.app/)** — a lightweight option when you just want a simple login in front of the app.

A typical setup is: reverse proxy (Nginx / Traefik / Caddy) terminates TLS, delegates authentication to one of the above via forward-auth, and proxies the authenticated request to KizunaShelf on `127.0.0.1:8787`. KizunaShelf itself stays bound to loopback and never sees an unauthenticated request.

## Related docs

- [docs/config.md](config.md) — full schema, per-runtime configuration, Settings behavior, and the complete environment-variable table.
- [docs/syncing.md](syncing.md) — how to sync a vault across devices.
