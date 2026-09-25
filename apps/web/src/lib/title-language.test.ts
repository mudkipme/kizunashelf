import { describe, expect, it } from "vitest";

import { entityTitle, isIso639TitleLanguage, titleLanguageLabel } from "./title-language";

describe("entityTitle", () => {
  const entity = { title: "Fallback", titles: { ja: "日本語", zh: "中文" } };

  it("returns the language's title when present", () => {
    expect(entityTitle(entity, "ja")).toBe("日本語");
    expect(entityTitle(entity, "zh")).toBe("中文");
  });

  it("falls back to the language-agnostic title otherwise", () => {
    expect(entityTitle(entity, "en")).toBe("Fallback");
    expect(entityTitle({ title: "Only", titles: {} }, "ja")).toBe("Only");
  });
});

describe("isIso639TitleLanguage", () => {
  it("accepts 2-3 lowercase letter codes only", () => {
    expect(isIso639TitleLanguage("en")).toBe(true);
    expect(isIso639TitleLanguage("jpn")).toBe(true);
    expect(isIso639TitleLanguage("e")).toBe(false);
    expect(isIso639TitleLanguage("engl")).toBe(false);
    expect(isIso639TitleLanguage("EN")).toBe(false);
    expect(isIso639TitleLanguage("zh-CN")).toBe(false);
    expect(isIso639TitleLanguage("")).toBe(false);
    expect(isIso639TitleLanguage(null)).toBe(false);
    expect(isIso639TitleLanguage(undefined)).toBe(false);
  });
});

describe("titleLanguageLabel", () => {
  it("resolves a human language name via Intl", () => {
    expect(titleLanguageLabel("ja")).toBe("Japanese");
    expect(titleLanguageLabel("en")).toBe("English");
  });
});
