import { describe, expect, it } from "vitest";

import type { TypeConfig } from "@/types/api";

import type { FieldConfig } from "./type-config";
import { filenameTitleField, resolveCreateBasename } from "./entity-create-form";

const field = (extra: Partial<FieldConfig>): FieldConfig =>
  ({ field: "x", fieldType: "text", ...extra }) as FieldConfig;

const typeConfig = (fields: FieldConfig[], extra: Partial<TypeConfig> = {}): TypeConfig =>
  ({ id: "movies", label: "Movies", path: "Movies", fields, ...extra }) as TypeConfig;

describe("filenameTitleField", () => {
  it("keys off the schema, never the field name", () => {
    // The title field is called `名前` — the name must not matter.
    const config = typeConfig([field({ field: "poster", fieldType: "image" }), field({ field: "名前", fieldType: "title" })]);
    expect(filenameTitleField(config)?.field).toBe("名前");
  });

  it("prefers the type's filename.titleLanguage, then titleRole, then the original-role field", () => {
    const jp = field({ field: "name_jp", fieldType: "title", titleLanguage: "ja" });
    const original = field({ field: "name", fieldType: "title", titleRole: "original" });
    expect(
      filenameTitleField(typeConfig([original, jp], { filename: { titleLanguage: "ja" } }))?.field,
    ).toBe("name_jp");
    expect(
      filenameTitleField(typeConfig([jp, original], { filename: { titleRole: "original" } }))?.field,
    ).toBe("name");
    expect(filenameTitleField(typeConfig([jp, original]))?.field).toBe("name");
  });

  it("returns undefined when the type has no title field", () => {
    expect(filenameTitleField(typeConfig([field({ field: "poster", fieldType: "image" })]))).toBeUndefined();
  });
});

describe("resolveCreateBasename (manual creation)", () => {
  const config = typeConfig([field({ field: "titel", fieldType: "title" })]);

  it("succeeds with the title-derived filename when the filename input is empty", () => {
    const resolution = resolveCreateBasename({
      typeConfig: config,
      frontmatter: { titel: "Fate/stay night" },
      manualBasename: "",
    });
    expect(resolution.canCreate).toBe(true);
    expect(resolution.source).toBe("title");
    // Forbidden characters are swapped for full-width, like the core's quick-add.
    expect(resolution.basename).toBe("Fate／stay night");
  });

  it("lets a manual filename override the title", () => {
    const resolution = resolveCreateBasename({
      typeConfig: config,
      frontmatter: { titel: "Some Title" },
      manualBasename: "  My File  ",
    });
    expect(resolution).toMatchObject({ canCreate: true, source: "manual", basename: "My File" });
  });

  it("blocks creation with no filename and no title instead of silently doing nothing", () => {
    const resolution = resolveCreateBasename({
      typeConfig: config,
      frontmatter: {},
      manualBasename: "",
    });
    expect(resolution.canCreate).toBe(false);
    expect(resolution.source).toBe("none");
  });

  it("blocks creation and reports the error for an invalid manual filename", () => {
    const resolution = resolveCreateBasename({
      typeConfig: config,
      frontmatter: { titel: "Fallback" },
      manualBasename: "bad:name",
    });
    expect(resolution.canCreate).toBe(false);
    expect(resolution.source).toBe("manual");
    expect(resolution.error).toMatch(/cannot contain/);
  });

  it("ignores an unusable title (whitespace or only forbidden characters)", () => {
    for (const title of ["   ", "...", ""]) {
      const resolution = resolveCreateBasename({
        typeConfig: config,
        frontmatter: { titel: title },
        manualBasename: "",
      });
      expect(resolution.canCreate).toBe(false);
    }
  });
});
