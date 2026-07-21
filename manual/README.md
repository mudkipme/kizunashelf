# KizunaShelf Manual

The user manual, built with [Zola](https://www.getzola.org/) (0.22+) and the [Goyo](https://github.com/hahwul/goyo) documentation theme (`themes/goyo`, a **git submodule** — run `git submodule update --init` after a fresh clone, and give any CI/deploy job submodule access).

```bash
cd manual
zola serve          # live preview at http://127.0.0.1:1111
zola build          # emits public/ (gitignored)
```

## Structure

- `content/_index.md` — the root renders `templates/home.html`. It's a fully standalone HTML page (inline CSS, no JS, no Goyo markup or styles — site templates shadow the theme, and this one extends nothing), with its assets in `static/assets/` (fonts + webp images; favicons and og-image are shared with the manual's `static/icons/` and `static/images/`). The header nav and a hero button link to `/introduction/`, and Goyo's own header wordmark links back to `/`.
- `introduction/` → `start/` → `concepts/` → `features/` → `cookbook/` → `guides/` → `reference/` → `faq/`, ordered by section `weight`. Sidebar, search (⌘K), and the dark/light toggle all come from the theme.
- `features/` has one page per feature (adding, editing, import, log activity, episodes, browse, relations, calendar & activity, lists, home, statistics, covers, schema, iOS). Screenshot placeholders use the `{{ screenshot(caption="…") }}` shortcode (`templates/shortcodes/screenshot.html`) — pass `desktop=`/`ios=` image paths to replace a placeholder with a real capture, or `platforms="ios"` for a single frame.
- **Terminology is standardized** in `concepts/terminology.md` — notably *entity* (never entry/item/record) for one thing in the library, *item* only for checklist/list items, *match* (not sync) for provider metadata. Follow it in every page.
- Stub pages carry a `<!-- TODO -->` outline at the top describing what to write.
- **Generated pages** — `reference/field-types.md`, `reference/providers.md`, and `reference/presets.md` are emitted from the Rust source by the `kizunashelf-docs` bin (`pnpm docs:generate` at the repo root) and carry a GENERATED banner; CI regenerates and diffs `manual/content/reference`, so edit the Rust source (types.rs / the provider registry / presets.rs / bin/docs.rs), never these files.

## Conventions

- One long line per paragraph — no hard-wrapping prose.
- Page titles live in front matter; don't start the body with an `# h1`.
- Internal links use Zola's `@/path/page.md#anchor` form so broken links fail the build. Zola slugifies `A / B / C` headings with single dashes (`a-b-c`), unlike GitHub's double.
- Goyo resolves `icon = "<name>"` (e.g. the nav's `book`) against its bundled Font Awesome 6 SVG set; to use a name it doesn't bundle, drop `<name>.svg` (FA6 free, solid) into `static/icons/` (site `static/` merges over the theme's).

## Planned (see also TODO comments in the pages)

- Translations (ja / zh-Hans / zh-Hant) once the English manual is done — the earlier translated intros and their `[languages.*]` / `nav_*` / lang-alias config were removed on this branch (`git log -- 'manual/content/_index.*.md'` recovers them); the old zh-Hant intro was a script conversion that needed a native review anyway.
- CI job: `zola build` (link check runs as part of the build) + deploy, with submodule checkout.
