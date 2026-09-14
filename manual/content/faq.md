---
title: "FAQ & troubleshooting"
sidebar_position: 7
---

## Why isn't my cover showing?

In the type's schema, check that your cover field is set to **Image** or **Image list**. If it points to an online image, the source may be unavailable; try [downloading a local copy](./features/covers.mdx).

## I edited frontmatter in Obsidian and KizunaShelf shows something odd

Refresh the library, then check that the field is configured in the [schema editor](./features/schema.mdx). A field can still be present in your file even if it is not shown where you expect in the app.

## Why won't an asset download from my NAS / LAN host?

Downloads from private network addresses are blocked by default. For a trusted image host on your own network, see `KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS` in the [server configuration](./reference/web-server.md#configuration).

## I got a "conflict" error when saving

The file changed after you opened it, possibly in another editor or on another device. Reload it, check the newer version, and apply your edit again. See [Avoiding sync conflicts](./guides/syncing.md#avoiding-sync-conflicts).

## My config didn't reach my other device

Check that your sync service includes `.yaml` and `.base` files, not only Markdown. See [Syncing your vault](./guides/syncing.md).

## Home is empty

Open Home and choose **Add suggested lists**, or pin an existing smart list. See [Home & smart lists](./features/home.mdx).
