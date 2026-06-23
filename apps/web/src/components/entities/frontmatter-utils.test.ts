import { describe, expect, it } from "vitest";

import {
  formatDateValue,
  formatSeasonValue,
  listDisplayValues,
  normalizeFrontmatter,
  numberOrString,
  parseDateValue,
  parseScalarValue,
  parseSeasonValue,
  stripWikilink,
  toWikilink,
  valueToText,
} from "./frontmatter-utils";

describe("parseScalarValue", () => {
  it("maps blank to null and recognizes booleans", () => {
    expect(parseScalarValue("")).toBeNull();
    expect(parseScalarValue("   ")).toBeNull();
    expect(parseScalarValue("true")).toBe(true);
    expect(parseScalarValue("false")).toBe(false);
  });

  it("parses canonical numbers but keeps non-canonical numeric strings", () => {
    expect(parseScalarValue("42")).toBe(42);
    expect(parseScalarValue("3.14")).toBe(3.14);
    // Leading zeros / exponent forms don't round-trip, so they stay strings
    // (otherwise an id like "007" would be silently renumbered).
    expect(parseScalarValue("007")).toBe("007");
    expect(parseScalarValue("1e3")).toBe("1e3");
  });

  it("returns the original (untrimmed) string for plain text", () => {
    expect(parseScalarValue("  hello ")).toBe("  hello ");
  });
});

describe("numberOrString", () => {
  it("returns null for empty, numbers for canonical numerics, strings otherwise", () => {
    expect(numberOrString("")).toBeNull();
    expect(numberOrString("42")).toBe(42);
    expect(numberOrString("007")).toBe("007");
    expect(numberOrString("abc")).toBe("abc");
  });
});

describe("valueToText", () => {
  it("renders scalars and JSON-encodes structured values", () => {
    expect(valueToText(null)).toBe("");
    expect(valueToText(undefined)).toBe("");
    expect(valueToText("x")).toBe("x");
    expect(valueToText(5)).toBe("5");
    expect(valueToText(true)).toBe("true");
    expect(valueToText(["a", "b"])).toBe('["a","b"]');
  });
});

describe("wikilinks", () => {
  it("strips a plain or aliased wikilink to its target", () => {
    expect(stripWikilink("[[Star Voyager]]")).toBe("Star Voyager");
    expect(stripWikilink("[[Star Voyager|SV]]")).toBe("Star Voyager");
    expect(stripWikilink("Plain")).toBe("Plain");
  });

  it("wraps a value, idempotently, and drops empties", () => {
    expect(toWikilink("Star Voyager")).toBe("[[Star Voyager]]");
    expect(toWikilink("[[Star Voyager]]")).toBe("[[Star Voyager]]");
    expect(toWikilink("[[Star Voyager|SV]]")).toBe("[[Star Voyager]]");
    expect(toWikilink("   ")).toBe("");
  });
});

describe("listDisplayValues", () => {
  it("normalizes a scalar or array into trimmed, non-empty display items", () => {
    expect(listDisplayValues(" a ", false)).toEqual(["a"]);
    expect(listDisplayValues(["a", "", "b"], false)).toEqual(["a", "b"]);
    expect(listDisplayValues("", false)).toEqual([]);
    expect(listDisplayValues(null, false)).toEqual([]);
  });

  it("strips wikilinks when the field is wikilink-shaped", () => {
    expect(listDisplayValues(["[[Beta]]", "[[Gamma|G]]"], true)).toEqual(["Beta", "Gamma"]);
  });
});

describe("season values", () => {
  it("parses the Chinese 季-suffixed, Japanese bare-kanji, and English shapes", () => {
    expect(parseSeasonValue("2023年春季")).toEqual({ kind: "season", year: "2023", season: "spring" });
    expect(parseSeasonValue("2020年秋季")).toEqual({ kind: "season", year: "2020", season: "autumn" });
    expect(parseSeasonValue("2023年春")).toEqual({ kind: "season", year: "2023", season: "spring" });
    expect(parseSeasonValue("2022年冬")).toEqual({ kind: "season", year: "2022", season: "winter" });
    expect(parseSeasonValue("Winter 2021")).toEqual({ kind: "season", year: "2021", season: "winter" });
    expect(parseSeasonValue("Fall 1999")).toEqual({ kind: "season", year: "1999", season: "autumn" });
  });

  it("falls back to raw when a year or season is missing", () => {
    expect(parseSeasonValue("2020")).toEqual({ kind: "raw", value: "2020" });
    expect(parseSeasonValue("sometime")).toEqual({ kind: "raw", value: "sometime" });
  });

  it("formats per language", () => {
    const row = { kind: "season", year: "2023", season: "spring" } as const;
    expect(formatSeasonValue(row, "zh")).toBe("2023年春季");
    expect(formatSeasonValue(row, "en")).toBe("Spring 2023");
    expect(formatSeasonValue(row, "ja")).toBe("2023年春");
  });

  it("round-trips every language form through parsing", () => {
    const row = { kind: "season", year: "2023", season: "spring" } as const;
    for (const lang of ["zh", "en", "ja"] as const) {
      expect(parseSeasonValue(formatSeasonValue(row, lang))).toEqual(row);
    }
  });
});

describe("date values", () => {
  it("parses valid ISO dates and rejects impossible ones", () => {
    expect(parseDateValue("2023-02-28")).toBeInstanceOf(Date);
    expect(parseDateValue("2023-02-30")).toBeUndefined(); // Feb 30 rolls over
    expect(parseDateValue("2023-13-01")).toBeUndefined();
    expect(parseDateValue("not a date")).toBeUndefined();
  });

  it("round-trips a date through format and parse", () => {
    const date = new Date(2023, 1, 5); // 2023-02-05 (local)
    expect(formatDateValue(date)).toBe("2023-02-05");
    expect(parseDateValue("2023-02-05")?.getTime()).toBe(date.getTime());
  });
});

describe("normalizeFrontmatter", () => {
  it("coerces unknown scalar shapes to strings while keeping known scalars and nesting", () => {
    const normalized = normalizeFrontmatter({
      title: "Star Voyager",
      rating: 5,
      watched: true,
      empty: null,
      tags: ["a", 2],
      nested: { a: 1 },
    });
    expect(normalized).toEqual({
      title: "Star Voyager",
      rating: 5,
      watched: true,
      empty: null,
      tags: ["a", 2],
      nested: { a: 1 },
    });
  });
});
