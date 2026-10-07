---
title: "Self-hosting"
description: "Connect the web app to a vault on your own server."
sidebar_position: 2
---

The web app serves one vault to a browser and is available as `ghcr.io/mudkipme/kizunashelf:latest`. Source code and development instructions are on [GitHub](https://github.com/mudkipme/kizunashelf).

## Run with Docker

Mount your vault folder at `/vault`:

```bash
docker run -p 127.0.0.1:8787:8787 \
  -v /path/to/vault:/vault \
  ghcr.io/mudkipme/kizunashelf:latest
```

Open `http://localhost:8787`. With these defaults, the app can read the vault but cannot edit it.

To enable setup and editing, add both write settings:

```bash
docker run -p 127.0.0.1:8787:8787 \
  -v /path/to/vault:/vault \
  -e KIZUNASHELF_CONTENT_WRITABLE=true \
  -e KIZUNASHELF_SETTINGS_WRITABLE=true \
  ghcr.io/mudkipme/kizunashelf:latest
```

The container also needs permission to write to the mounted folder. For a read-only vault, leave the settings disabled and mount it with `/path/to/vault:/vault:ro`.

On Linux, add `--user "$(id -u):$(id -g)"` to run the container with your user and group IDs so newly created vault files belong to you. That user must have access to the mounted folder.

## Configuration

The server uses environment variables. See the [server reference](../reference/web-server.md#configuration) for the full list, including provider credentials, caches, and logs.

The library's types and fields live in the vault and are edited through [Settings](../features/schema.mdx).

## Remote access {#authentication}

For access beyond your own machine, use an HTTPS reverse proxy and authentication. Keep the application port on a private network. KizunaShelf has an optional single-user password login; a proxy can also provide authentication.

See the [authentication reference](../reference/web-server.md#authentication) for password setup and proxy requirements.

## Keep your library safe

Back up the mounted vault folder, including its settings, lists, and covers. Hosting the web app does not sync your other devices; see [Syncing your vault](../guides/syncing.md).
