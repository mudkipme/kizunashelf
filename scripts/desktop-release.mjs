import { appendFileSync, copyFileSync, mkdirSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const config = JSON.parse(readFileSync("apps/desktop/src-tauri/tauri.conf.json", "utf8"));

function requireValues(names) {
  const missing = names.filter((name) => !process.env[name]);
  if (missing.length) throw new Error(`Missing release configuration: ${missing.join(", ")}`);
}

function prepare() {
  const { version } = config;
  const workspacePackage = readFileSync("Cargo.toml", "utf8").split("[workspace.package]")[1];
  const rustVersion = workspacePackage?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  const packagePaths = ["package.json", "apps/desktop/package.json"];
  const packageVersions = packagePaths.map(
    (path) => JSON.parse(readFileSync(path, "utf8")).version,
  );
  if ([rustVersion, ...packageVersions].some((value) => value !== version)) {
    throw new Error(
      "Desktop, workspace Rust, root package and desktop package versions must match.",
    );
  }

  const publish = process.env.PUBLISH === "true";
  const sign = publish || process.env.SIGN === "true";
  if (publish && process.env.GITHUB_REF !== `refs/tags/v${version}`) {
    throw new Error(`Publish from the matching version tag v${version}.`);
  }
  if (sign) {
    requireValues([
      "APPLE_CERTIFICATE",
      "APPLE_CERTIFICATE_PASSWORD",
      "APPLE_SIGNING_IDENTITY",
      "APPLE_ID",
      "APPLE_PASSWORD",
      "APPLE_TEAM_ID",
    ]);
    if (!process.env.APPLE_SIGNING_IDENTITY.startsWith("Developer ID Application:")) {
      throw new Error("APPLE_SIGNING_IDENTITY must be a Developer ID Application identity.");
    }
  }

  const signpathNames = [
    "SIGNPATH_API_TOKEN",
    "SIGNPATH_ORGANIZATION_ID",
    "SIGNPATH_PROJECT_SLUG",
    "SIGNPATH_SIGNING_POLICY_SLUG",
    "SIGNPATH_APP_CONFIGURATION_SLUG",
    "SIGNPATH_INSTALLER_CONFIGURATION_SLUG",
  ];
  const signpath = sign && signpathNames.some((name) => process.env[name]);
  if (signpath) requireValues(signpathNames);
  if (publish && !signpath && process.env.WINDOWS_SIGNING_REQUIRED !== "false") {
    throw new Error(
      "Configure SignPath before publishing, or explicitly set WINDOWS_SIGNING_REQUIRED=false to release unsigned Windows installers.",
    );
  }
  appendFileSync(
    process.env.GITHUB_OUTPUT,
    `version=${version}\nsign=${sign}\nsignpath=${signpath}\n`,
  );
}

function collect() {
  requireValues(["BUILD_TARGET", "BUILD_PLATFORM", "BUILD_ARCH", "BUILD_BUNDLE", "RELEASE_DIR"]);
  const { BUILD_TARGET, BUILD_PLATFORM, BUILD_ARCH, BUILD_BUNDLE, RELEASE_DIR } = process.env;
  const extensions = { dmg: ".dmg", appimage: ".AppImage", nsis: ".exe" };
  const extension = extensions[BUILD_BUNDLE];
  if (!extension) throw new Error(`Unsupported bundle: ${BUILD_BUNDLE}`);
  const directory = join("target", BUILD_TARGET, "release", "bundle", BUILD_BUNDLE);
  const files = readdirSync(directory).filter((name) => name.endsWith(extension));
  if (files.length !== 1)
    throw new Error(`Expected one ${extension} in ${directory}, got ${files.length}.`);
  const suffix = BUILD_BUNDLE === "nsis" ? "-setup" : "";
  const name = `KizunaShelf_${config.version}_${BUILD_PLATFORM}_${BUILD_ARCH}${suffix}${extension}`;
  mkdirSync(RELEASE_DIR, { recursive: true });
  copyFileSync(join(directory, files[0]), join(RELEASE_DIR, name));
  console.log(`Collected ${name}`);
}

const command = process.argv[2];
if (command === "prepare") prepare();
else if (command === "collect") collect();
else throw new Error("Usage: node scripts/desktop-release.mjs <prepare|collect>");
