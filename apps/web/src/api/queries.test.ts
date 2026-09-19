import { describe, expect, it } from "vitest";

import { queryKeys } from "./queries";

describe("queryKeys", () => {
  it("roots every key at its own entry name, so invalidating a QueryRoot matches it", () => {
    for (const [name, entry] of Object.entries(queryKeys)) {
      const key =
        typeof entry === "function"
          ? (entry as (...args: unknown[]) => readonly unknown[])(
              ...Array.from({ length: entry.length }, () => undefined),
            )
          : entry;
      expect(key[0], name).toBe(name);
    }
  });
});
