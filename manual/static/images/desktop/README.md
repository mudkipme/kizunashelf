# Desktop manual captures

Captured on 2026-09-05 from the installed `/Applications/KizunaShelf.app` on macOS. These are native Retina app-window captures: **2704 × 1786 pixels for a 1352 × 893-point window (2×)**, stored as lossless PNGs with their original color profile. The screen-mirroring badge was replaced with the standard red, yellow, and green window controls. Only the 136 × 44-pixel badge area at `(10, 10)` was repaired; all other pixels retain the native capture. No images were upscaled. The app uses an existing vault with custom English labels and multilingual content; the recipes use the English built-in presets, so their labels can differ.

| File | View |
| --- | --- |
| `home.png` | Coming up and Watching Anime shelf |
| `library.png` | Anime grid |
| `filters.png` | Anime filtered by Wishlist |
| `quick-capture.png` | Provider search results, before creating an entity |
| `manual-add.png` | Manual form, before creating an entity |
| `editing.png` | Existing entity's frontmatter editor |
| `matching.png` | Selected provider metadata comparison, before applying |
| `episodes.png` | Episode checklist with scheduled and completed dates |
| `log.png` | Log preview, before submitting |
| `relations.png` | Game's incoming and outgoing connections |
| `calendar.png` | September 2026 calendar |
| `activity.png` | Recent activity |
| `list.png` | Existing curated list |
| `smart-list.png` | Existing smart list in the criteria editor |
| `statistics.png` | Totals and distributions |
| `review.png` | Cleanup queues and batch cover controls |
| `import.png` | Import source picker, before planning or committing |
| `schema.png` | Settings type overview |
| `type-editor.png` | Type configuration dialog |

## Updating captures

Open the installed desktop app, navigate to the named view, and wait for content and images to settle. Use macOS’s native `screencapture -x -o -l <window-id> <file>.png` on a Retina display to capture the app window. Verify the output is 2704 × 1786 pixels; the computer-use preview is downscaled and should not be used as the final asset. If mirroring replaces the traffic lights, repair only that badge area at native resolution, preserving the background and color profile. Keep credentials, absolute vault paths, unrelated apps, and dialogs with private information out of frame. Preview actions do not require saving schema changes, editing library content, committing imports, or starting a cover batch.

Use a caption that describes the captured stage accurately. Replace the corresponding file and update its `desktopAlt`, then check the rendered page at desktop and narrow widths. `<Screenshot>` links to the full-size capture. Feature pages use `platforms="both"` to keep a space for an iOS capture; add it with the `ios` and `iosAlt` props when ready. See the [manual README](../../../README.md#adding-screenshots) for placement and example conventions.
