import { describe, expect, it } from "vitest";

import { isLocalAsset, isRemoteAsset, resolveAssetSrc } from "./asset-src";

// These run in the node test environment (no `window`), i.e. the web runtime
// path — local assets resolve through the `/api/assets` route, not `kizasset`.

describe("resolveAssetSrc", () => {
  it("returns undefined for empty input", () => {
    expect(resolveAssetSrc(null)).toBeUndefined();
    expect(resolveAssetSrc(undefined)).toBeUndefined();
    expect(resolveAssetSrc("   ")).toBeUndefined();
  });

  it("passes remote and inline URLs through unchanged", () => {
    expect(resolveAssetSrc("https://example.com/a.jpg")).toBe("https://example.com/a.jpg");
    expect(resolveAssetSrc("http://example.com/a.jpg")).toBe("http://example.com/a.jpg");
    expect(resolveAssetSrc("data:image/png;base64,AAAA")).toBe("data:image/png;base64,AAAA");
    expect(resolveAssetSrc("blob:abc")).toBe("blob:abc");
  });

  it("serves a local vault path through the API asset route, URL-encoding each segment", () => {
    expect(resolveAssetSrc("Assets/Anime/Foo Bar/cover.jpg")).toBe(
      "/api/assets/Assets/Anime/Foo%20Bar/cover.jpg",
    );
    // Leading and duplicate slashes are dropped.
    expect(resolveAssetSrc("/Assets//cover.png")).toBe("/api/assets/Assets/cover.png");
  });
});

describe("isLocalAsset / isRemoteAsset", () => {
  it("classifies local vault paths", () => {
    expect(isLocalAsset("Assets/Anime/cover.jpg")).toBe(true);
    expect(isRemoteAsset("Assets/Anime/cover.jpg")).toBe(false);
  });

  it("classifies remote URLs", () => {
    expect(isRemoteAsset("https://example.com/a.jpg")).toBe(true);
    expect(isLocalAsset("https://example.com/a.jpg")).toBe(false);
  });

  it("treats data/blob URLs as neither local nor remote", () => {
    expect(isLocalAsset("data:image/png;base64,AAAA")).toBe(false);
    expect(isRemoteAsset("data:image/png;base64,AAAA")).toBe(false);
    expect(isRemoteAsset("blob:abc")).toBe(false);
  });

  it("returns false for empty input", () => {
    expect(isLocalAsset(null)).toBe(false);
    expect(isRemoteAsset(undefined)).toBe(false);
  });
});
