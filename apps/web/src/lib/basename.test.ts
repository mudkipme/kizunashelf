import { describe, expect, it } from "vitest";

import { basenameValidationError, normalizeBasename } from "./basename";

describe("normalizeBasename", () => {
  it("trims surrounding whitespace", () => {
    expect(normalizeBasename("  Star Voyager  ")).toBe("Star Voyager");
  });
});

describe("basenameValidationError", () => {
  it("accepts a plain basename", () => {
    expect(basenameValidationError("Star Voyager")).toBeUndefined();
    // Leading/trailing whitespace is normalized before validating.
    expect(basenameValidationError("  Star Voyager  ")).toBeUndefined();
  });

  it("rejects an empty or whitespace-only name", () => {
    expect(basenameValidationError("")).toBe("Filename cannot be empty.");
    expect(basenameValidationError("   ")).toBe("Filename cannot be empty.");
  });

  it("rejects the dot directories", () => {
    expect(basenameValidationError(".")).toBe("Filename cannot be . or ...");
    expect(basenameValidationError("..")).toBe("Filename cannot be . or ...");
  });

  it("rejects a trailing .md (the caller stores the basename only)", () => {
    expect(basenameValidationError("Star Voyager.md")).toBe("Enter the basename without .md.");
    // Case-insensitive on the extension.
    expect(basenameValidationError("Star Voyager.MD")).toBe("Enter the basename without .md.");
  });

  it("rejects forbidden filesystem characters and control chars", () => {
    for (const bad of ["a/b", "a\\b", "a:b", "a*b", "a?b", 'a"b', "a<b", "a>b", "a|b"]) {
      expect(basenameValidationError(bad)).toMatch(/cannot contain/);
    }
    expect(basenameValidationError("ab")).toMatch(/cannot contain/);
  });
});
