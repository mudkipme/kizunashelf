---
title: "Quickstart"
description: "Create your first vault on desktop, iOS, or self-hosted web."
sidebar_position: 1
---

A **vault** is the folder where KizunaShelf keeps your library. You can create an empty one or use a folder you already keep in Obsidian.

## iOS

1. Install KizunaShelf from [TestFlight](https://testflight.apple.com/join/hE7k3sWd).
2. Create a vault under **On My iPhone** or **On My iPad**, or open an existing folder from Files.
3. For a new vault, choose your preferred language and the types you want to collect. Start with **Anime**, **Games**, or **Music Albums**, for example.
4. Finish setup. Your library opens with suggested smart lists on Home.

You can add types and change their fields later in **More → Vault Schema**. An existing KizunaShelf vault opens directly, without repeating setup.

## Desktop

1. Download the desktop alpha for macOS, Linux, or Windows from [GitHub Releases](https://github.com/mudkipme/kizunashelf/releases). Choose the amd64 build for Intel/AMD machines or arm64 for Apple Silicon and other ARM machines.
2. On macOS, open the DMG and drag KizunaShelf to Applications. On Windows, run the installer. On Linux, make the AppImage executable (`chmod +x KizunaShelf_*.AppImage`), then open it.
3. Open or create a vault folder. For a new vault, choose your preferred language and the types you want to collect.

Schema settings are under **Settings**. The desktop release is an alpha; feedback and bug reports are welcome on [GitHub Issues](https://github.com/mudkipme/kizunashelf/issues).

## Self-hosted web

Run the published Docker image with setup and editing enabled:

```bash
mkdir -p vault
docker run -p 127.0.0.1:8787:8787 \
  -v "$PWD/vault:/vault" \
  -e KIZUNASHELF_CONTENT_WRITABLE=true \
  -e KIZUNASHELF_SETTINGS_WRITABLE=true \
  ghcr.io/mudkipme/kizunashelf:latest
```

Open `http://localhost:8787/` and complete setup. Your library is stored in the local `vault` folder; to use an existing vault, replace `$PWD/vault` with its absolute path. See [Self-hosting](./self-hosting.md) for read-only mode, configuration, and remote access.

## Set up metadata sources

For richer metadata searches on **web and desktop**, configure your own credentials for the providers you use. iOS version includes built-in credentials for most providers.

| What you collect | Provider | Credentials |
| --- | --- | --- |
| Games | [IGDB](../reference/providers.md#igdb) | Client ID and client secret |
| Film and TV | [TMDB](../reference/providers.md#tmdb) | API key |
| TV and episode details | [TheTVDB](../reference/providers.md#thetvdb) | API key; a PIN may also be needed for your account |

On **desktop**, open **Settings → Provider Credentials**, enter the credentials, and select **Save credentials**. They are stored in your system keychain.

On **self-hosted web**, pass the provider's environment variables to the server or Docker container and restart it: `KIZUNASHELF_IGDB_CLIENT_ID` and `KIZUNASHELF_IGDB_CLIENT_SECRET`, `KIZUNASHELF_TMDB_API_KEY`, or `KIZUNASHELF_TVDB_API_KEY` (plus `KIZUNASHELF_TVDB_PIN` if needed). See the [provider reference](../reference/providers.md) for the exact requirements and the [server configuration](../reference/web-server.md#configuration) for deployment settings.

You can still browse and edit your vault without these credentials. Providers such as Bangumi, AniList, and Open Library work without an API key.

## Add your first entity

An **entity** is one thing in your library, such as *Steins;Gate 0 (Anime)*.

1. Open **Quick Capture** and choose **Anime**.
2. Search for *Steins;Gate 0*, then select the matching result.
3. Add it to your library. Open it to record your status, rating, or notes.

See [Adding an entity](../features/adding.mdx) for manual entry and search requirements. To bring in an existing collection instead, use [Batch import](../features/import.mdx).

Your Home lists update as you add and edit entities. You can [pin your own smart lists or recreate suggestions](../features/home.mdx) at any time.
