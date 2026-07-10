#!/usr/bin/env bash
# Builds the kizunashelf-ffi static library for iOS device + simulator, generates
# the UniFFI Swift bindings, and packages the native code into
# KizunaFFI.xcframework for the KizunaCore Swift package in ../kizunashelf-ios.
#
# UniFFI produces two halves:
#   - a low-level clang module (kizunashelf_ffiFFI.h + module.modulemap) packaged
#     INSIDE the xcframework alongside the static libs, and
#   - a high-level Swift file (kizunashelf_ffi.swift) copied into the KizunaCore
#     target's Generated/ dir. Both are regenerated from the freshly built library
#     so their interface checksums always match.
#
# Run on macOS with Xcode + the Rust iOS targets installed:
#   rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
#
# Usage: scripts/build-ios.sh [ios_repo_root]
#   ios_repo_root defaults to ../kizunashelf-ios
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATE="kizunashelf-ffi"
LIB="libkizunashelf_ffi.a"
IOS_ROOT="${1:-$REPO_ROOT/../kizunashelf-ios}"
PKG="$IOS_ROOT/Packages/KizunaCore"
XCFRAMEWORK="$PKG/Frameworks/KizunaFFI.xcframework"
SWIFT_GEN_DIR="$PKG/Sources/KizunaCore/Generated"

DEVICE_TARGET="aarch64-apple-ios"
SIM_TARGETS=("aarch64-apple-ios-sim" "x86_64-apple-ios")

# Where cargo writes build artifacts. Honor CARGO_TARGET_DIR (e.g. a persistent CI
# cache dir outside the checkout) so the artifact reads below match where cargo
# actually put them; otherwise the default per-workspace `target/`.
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO_ROOT/target}"

# Pin a modern iOS deployment target for both rustc's link step and the C/asm
# builds (aws-lc-sys, pulled in by reqwest's rustls). Without this, rustc links
# against the iOS 10 default while the SDK compiles aws-lc for a much newer iOS,
# leaving stack-probe builtins like `___chkstk_darwin` undefined. Matches the
# KizunaCore SwiftPM platform (.iOS(.v17), required by the app's @Observable use).
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"

BUILD_DIR="$(mktemp -d)"
HEADERS_DIR="$BUILD_DIR/Headers"
BINDINGS_DIR="$BUILD_DIR/bindings"
trap 'rm -rf "$BUILD_DIR"' EXIT

echo "==> Building $CRATE (release) for iOS targets"
for target in "$DEVICE_TARGET" "${SIM_TARGETS[@]}"; do
    echo "    - $target"
    cargo build --release -p "$CRATE" --target "$target" --manifest-path "$REPO_ROOT/Cargo.toml"
done

DEVICE_LIB="$TARGET_DIR/$DEVICE_TARGET/release/$LIB"

echo "==> Generating UniFFI bindings (Swift + C module) from the built library"
cargo run -q -p "$CRATE" --bin uniffi-bindgen -- generate \
    --library "$DEVICE_LIB" \
    --language swift \
    --out-dir "$BINDINGS_DIR"

# Headers + module map go inside the xcframework; the modulemap must be named
# `module.modulemap` for SPM/Xcode to pick it up (module name stays the same).
mkdir -p "$HEADERS_DIR"
cp "$BINDINGS_DIR/kizunashelf_ffiFFI.h" "$HEADERS_DIR/"
cp "$BINDINGS_DIR/kizunashelf_ffiFFI.modulemap" "$HEADERS_DIR/module.modulemap"

# The high-level Swift binding goes into the KizunaCore target sources.
mkdir -p "$SWIFT_GEN_DIR"
cp "$BINDINGS_DIR/kizunashelf_ffi.swift" "$SWIFT_GEN_DIR/"

# Generate the OpenAPI spec the swift-openapi-generator plugin reads, collapsing
# `anyOf: [X, {type: null}]` (schemars' Option<NamedType> encoding) down to `X`.
# The bare `{type: null}` branch makes swift-openapi-generator drop the whole
# property; the canonical spec (web/orval) keeps the null branch.
echo "==> Generating collapsed OpenAPI spec for swift-openapi-generator"
cargo run -q -p kizunashelf --bin kizunashelf-schema --manifest-path "$REPO_ROOT/Cargo.toml" -- \
    --collapse-nullable-refs "$PKG/Sources/KizunaCore/openapi.json"

echo "==> Creating fat simulator library"
SIM_LIBS=()
for target in "${SIM_TARGETS[@]}"; do
    SIM_LIBS+=("$TARGET_DIR/$target/release/$LIB")
done
SIM_FAT="$BUILD_DIR/sim-$LIB"
lipo -create "${SIM_LIBS[@]}" -output "$SIM_FAT"

echo "==> Assembling $XCFRAMEWORK"
rm -rf "$XCFRAMEWORK"
mkdir -p "$PKG/Frameworks"
xcodebuild -create-xcframework \
    -library "$DEVICE_LIB" -headers "$HEADERS_DIR" \
    -library "$SIM_FAT" -headers "$HEADERS_DIR" \
    -output "$XCFRAMEWORK"

echo "==> Done"
echo "    xcframework: $XCFRAMEWORK"
echo "    swift glue:  $SWIFT_GEN_DIR/kizunashelf_ffi.swift"
