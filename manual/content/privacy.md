---
title: "Privacy"
description: "KizunaShelf collects no data. The only thing that ever leaves your device is what you send to the metadata providers you configured."
sidebar_position: 8
---

KizunaShelf is built with full respect for your privacy. **KizunaShelf collects no data**: there are no accounts, no analytics, no telemetry, and no network connections to the developer. Your library is plain Markdown files in a folder you own, and everything KizunaShelf derives from them, indexes, statistics, calendars, is computed on your device (or on the server you host yourself) and stays there.

## External metadata providers

Some features talk to third-party metadata providers, but only the providers your schema maps, and only when you use those features. When you search in [Quick Capture](./features/adding.mdx), your search query is sent to the configured external providers so they can return matches; the same applies when you match an existing entity, sync episodes, run an import, or download a cover the provider hosts. Requests include your language preference for providers that localize results, and they go directly from your device or self-hosted server to the provider. Nothing passes through the developer.

These providers are third parties, not affiliated with KizunaShelf, and whatever you send them is handled under their own privacy policies. If you'd rather not contact a provider, don't map it in your schema: every external integration is opt-in, and KizunaShelf works fully without any of them.

## Self-hosting

The web app is self-hosted, so your data lives on whatever machine runs the server. Hosting it on your own local hardware protects your data best; if you host it on the internet instead, you are subject to your hosting provider's privacy policy. Either way, connect only via HTTPS or a local network.
