+++
title = "Downloading covers locally"
description = "Turn remote cover URLs into files in your vault — one entity at a time or in batch."
weight = 12
+++

A cover referenced by URL disappears when the remote host does. Downloading covers **into the vault** (under the configured `assetRoot`) makes them yours: they sync with the vault, show up offline, and survive the provider.

## How covers get local

- **[Quick Capture](@/features/adding.md)** downloads the matched candidate's cover at creation time (fail-safe — a failed download never blocks the entity).
- **[Matching](@/features/editing.md#matching-external-metadata)** an existing entity offers the cover among the metadata to apply.
- **The batch cover downloader** sweeps many at once: it finds image fields still holding remote URLs and downloads them, over one type or the whole library. You'll meet it on the [import](@/features/import.md) Review page (imports deliberately keep remote URLs) and from the [cleanup queues](@/features/statistics.md#review-cleanup-queues-library-diagnosis).

{{ screenshot(caption="The batch cover downloader working through a library of remote URLs.") }}

The downloaded file lands in `assetRoot`, the image field is updated to the vault-relative path, and the write is atomic like every other content write.

## Safety rails

Download URLs are validated against an **SSRF guard**: private, loopback, and link-local addresses are blocked, and redirects are re-validated per hop. If your covers legitimately live on a LAN host (a NAS, a self-hosted image server), opt in with `KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS` on the self-hosted web app.

Covers can be the largest part of a vault — worth remembering when picking a [sync method](@/guides/syncing.md).
