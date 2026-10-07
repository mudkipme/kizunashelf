# Contributing to KizunaShelf

Start with the [README](README.md) for local setup and [ARCHITECTURE.md](ARCHITECTURE.md) for the shared Rust core, schema-driven behavior, vault filesystem abstraction, and generated API contract.

Use Node.js 24, the pnpm version declared in `package.json`, and a stable Rust toolchain with rustfmt and Clippy. Install JavaScript dependencies with `pnpm install --frozen-lockfile`. Browser tests need Chromium: `pnpm --filter @kizunashelf/web exec playwright install --with-deps chromium`.

Keep changes focused and describe the user-visible behavior and relevant validation in your pull request. Add regression coverage for behavior changes. UI strings use Lingui; field names and schema labels are user data. All vault I/O must use the Rust `Vfs` abstraction.

Before submitting, run the checks in [.github/workflows/ci.yml](.github/workflows/ci.yml). Regenerate the API contract after API changes, translation catalogs after UI-copy or formatting changes, and reference docs after schema/provider/preset changes. Commit the resulting generated files. The manual build checks internal links and anchors.

Use synthetic vaults and credentials in examples and tests. Do not include a real vault, access tokens, or private screenshots in a pull request. Report vulnerabilities through the [security policy](SECURITY.md).

Contributions to the project's original code are under [MPL-2.0](LICENSE). Preserve the licenses and notices of third-party assets.
