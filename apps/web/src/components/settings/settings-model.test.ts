import { describe, expect, it, vi } from "vitest";

import type { FieldType } from "@/lib/type-config";
import type {
  EntityTypeConfig,
  ExternalProviderCatalog,
  FieldConfig,
  VaultConfig,
} from "@/types/api";

import {
  arrayEditor,
  cleanVaultConfig,
  defaultVaultConfig,
  joinPath,
  normalizeVaultConfig,
  relativeToBase,
  replaceArray,
} from "./settings-model";

describe("replaceArray", () => {
  it("replaces the item at the index without mutating the input", () => {
    const input = [1, 2, 3];
    expect(replaceArray(input, 1, 9)).toEqual([1, 9, 3]);
    expect(input).toEqual([1, 2, 3]);
  });

  it("returns an equivalent array when the index is out of range", () => {
    expect(replaceArray([1, 2, 3], 5, 9)).toEqual([1, 2, 3]);
  });
});

describe("arrayEditor", () => {
  it("append calls onChange with the item added at the end", () => {
    const onChange = vi.fn();
    arrayEditor([1, 2], onChange).append(3);
    expect(onChange).toHaveBeenCalledWith([1, 2, 3]);
  });

  it("update calls onChange with the item replaced at the index", () => {
    const onChange = vi.fn();
    arrayEditor([1, 2, 3], onChange).update(1, 9);
    expect(onChange).toHaveBeenCalledWith([1, 9, 3]);
  });

  it("remove calls onChange with the item at the index dropped", () => {
    const onChange = vi.fn();
    arrayEditor([1, 2, 3], onChange).remove(0);
    expect(onChange).toHaveBeenCalledWith([2, 3]);
  });

  it("does not mutate the bound array", () => {
    const values = [1, 2, 3];
    const editor = arrayEditor(values, () => {});
    editor.append(4);
    editor.update(0, 9);
    editor.remove(1);
    expect(values).toEqual([1, 2, 3]);
  });
});

describe("joinPath", () => {
  it("joins base and path with a single slash, trimming duplicates", () => {
    expect(joinPath("Assets", "covers")).toBe("Assets/covers");
    expect(joinPath("Assets/", "/covers")).toBe("Assets/covers");
  });

  it("returns the non-empty side when the other is empty", () => {
    expect(joinPath("", "covers")).toBe("covers");
    expect(joinPath("Assets", "")).toBe("Assets");
  });
});

describe("relativeToBase", () => {
  it("strips the base prefix from a contained path", () => {
    expect(relativeToBase("Taxonomy/Anime", "Taxonomy")).toBe("Anime");
    expect(relativeToBase("Taxonomy/Anime", "Taxonomy/")).toBe("Anime");
  });

  it("returns empty when the path equals the base", () => {
    expect(relativeToBase("Taxonomy", "Taxonomy")).toBe("");
  });

  it("returns the path unchanged when it isn't under the base", () => {
    expect(relativeToBase("Other/Anime", "Taxonomy")).toBe("Other/Anime");
    // A shared prefix that isn't a path segment boundary is not treated as contained.
    expect(relativeToBase("TaxonomyX/Anime", "Taxonomy")).toBe("TaxonomyX/Anime");
  });
});

// --- Config (de)serialization ------------------------------------------------

// `field`/`fieldType` are required; everything else is filled per-case. Casts
// keep the loose generated input types from fighting the fixtures.
function field(extra: Partial<FieldConfig> & { fieldType: FieldType }): FieldConfig {
  return { field: "f", ...extra } as FieldConfig;
}

function type(extra: Partial<EntityTypeConfig>): EntityTypeConfig {
  return {
    id: "anime",
    label: "Anime",
    icon: "",
    path: "Anime",
    externalPriority: [],
    filename: null,
    bodySections: [],
    fields: [],
    ...extra,
  } as EntityTypeConfig;
}

function vault(extra: Partial<VaultConfig>): VaultConfig {
  return {
    taxonomyRoot: "Taxonomy",
    assetRoot: "Assets",
    dailyNotes: null,
    home: null,
    types: [],
    ...extra,
  } as VaultConfig;
}

function catalogWith(providerId: string): ExternalProviderCatalog {
  return {
    providers: [
      {
        id: providerId,
        label: providerId,
        fields: [{ field: "name", label: "Name" }],
        types: [],
        defaultExternalTypes: [],
        credentials: [],
        searchSupported: true,
      },
    ],
  };
}

// `cleanVaultConfig` always returns `{ vault }`; narrow it for the assertions.
function cleanedVault(config: VaultConfig, catalog?: ExternalProviderCatalog) {
  const { vault: cleaned } = cleanVaultConfig(config, catalog);
  if (!cleaned) throw new Error("expected a cleaned vault");
  return cleaned;
}

describe("normalizeVaultConfig", () => {
  it("seeds the full defaults when the vault config is absent", () => {
    expect(normalizeVaultConfig()).toEqual(defaultVaultConfig());
  });

  it("preserves disabled (null) daily notes and home for an existing config", () => {
    const result = normalizeVaultConfig(
      vault({ taxonomyRoot: "T", dailyNotes: null, home: null, types: [] }),
    );
    expect(result.dailyNotes).toBeNull();
    expect(result.home).toBeNull();
    expect(result.types).toEqual([]);
    expect(result.taxonomyRoot).toBe("T");
  });

  it("fills field-level defaults (seasonLanguage, null roles, empty lists)", () => {
    const result = normalizeVaultConfig(
      vault({ types: [type({ fields: [field({ field: "title", fieldType: "title" })] })] }),
    );
    const [entityType] = result.types;
    expect(entityType).toMatchObject({ icon: "", externalPriority: [], filename: null, bodySections: [] });
    expect(entityType.fields[0]).toMatchObject({
      field: "title",
      fieldType: "title",
      displayName: "",
      titleRole: null,
      externalFields: [],
      enumOptions: [],
      seasonLanguage: "zh",
      externalRef: "",
      relationType: "",
    });
  });
});

describe("cleanVaultConfig", () => {
  it("strips empty optionals to undefined", () => {
    const cleaned = cleanedVault(vault({ assetRoot: "", dailyNotes: null, home: null, types: [] }));
    expect(cleaned.assetRoot).toBeUndefined();
    expect(cleaned.dailyNotes).toBeUndefined();
    expect(cleaned.home).toBeUndefined();
    expect(cleaned.types).toEqual([]);
  });

  it("drops fields with an empty name", () => {
    const cleaned = cleanedVault(
      vault({
        types: [
          type({
            fields: [
              field({ field: "  ", fieldType: "text" }),
              field({ field: "title", fieldType: "title" }),
            ],
          }),
        ],
      }),
    );
    expect(cleaned.types[0].fields.map((item) => item.field)).toEqual(["title"]);
  });

  it("round-trips the daily-note template + log and per-type log", () => {
    const cleaned = cleanedVault(
      normalizeVaultConfig(
        vault({
          dailyNotes: {
            paths: ["Daily Notes"],
            template: "Templates/Daily.md",
            log: { section: "Inputs", lineFormat: "- {title} {note}" },
          },
          types: [
            type({ id: "anime", log: { lineFormat: "- {title} {note} #Anime" } }),
            type({ id: "franchise" }),
          ],
        }),
      ),
    );
    expect(cleaned.dailyNotes).toMatchObject({
      template: "Templates/Daily.md",
      log: { section: "Inputs", lineFormat: "- {title} {note}" },
    });
    expect(cleaned.types[0].log).toMatchObject({ lineFormat: "- {title} {note} #Anime" });
    // A type with no `log` block stays not loggable.
    expect(cleaned.types[1].log).toBeUndefined();
  });

  it("drops an empty global log but keeps an enabled-but-empty per-type log", () => {
    const cleaned = cleanedVault(
      normalizeVaultConfig(
        vault({
          dailyNotes: { paths: ["Daily Notes"], log: { section: "", lineFormat: "" } },
          types: [type({ id: "anime", log: { section: "", lineFormat: "" } })],
        }),
      ),
    );
    // An empty global default carries no config → dropped.
    expect(cleaned.dailyNotes?.log).toBeUndefined();
    // Presence of a per-type `log` is the opt-in, so it survives even when empty.
    expect(cleaned.types[0].log).toBeDefined();
  });

  it("round-trips an enum status field's role and value mapping", () => {
    const cleaned = cleanedVault(
      normalizeVaultConfig(
        vault({
          types: [
            type({
              fields: [
                field({
                  field: "state",
                  fieldType: "enum",
                  enumOptions: ["Backlog", "Active", "Completed"],
                  enumRole: "status",
                  statusValues: {
                    planning: ["Backlog"],
                    ongoing: ["Active"],
                    completed: ["Completed"],
                    dropped: [],
                  },
                }),
              ],
            }),
          ],
        }),
      ),
    );
    // The role and mapping survive both the load-normalize and save-clean passes
    // (a regression here silently wipes status config on any settings save).
    expect(cleaned.types[0].fields[0]).toMatchObject({
      enumRole: "status",
      statusValues: {
        planning: ["Backlog"],
        ongoing: ["Active"],
        completed: ["Completed"],
      },
    });
  });

  it("drops the status role and mapping from a non-enum field", () => {
    const cleaned = cleanedVault(
      vault({
        types: [
          type({
            fields: [
              field({
                field: "note",
                fieldType: "text",
                enumRole: "status",
                statusValues: { ongoing: ["x"] },
              }),
            ],
          }),
        ],
      }),
    );
    expect(cleaned.types[0].fields[0].enumRole).toBeUndefined();
    expect(cleaned.types[0].fields[0].statusValues).toBeUndefined();
  });

  it("emits only the props relevant to each field type", () => {
    const cleaned = cleanedVault(
      vault({
        types: [
          type({
            fields: [
              field({ field: "genres", fieldType: "enumList", enumOptions: ["A", "B"] }),
              field({ field: "aired", fieldType: "season", seasonLanguage: "ja" }),
            ],
          }),
        ],
      }),
    );
    const [genres, aired] = cleaned.types[0].fields;
    expect(genres).toMatchObject({ enumOptions: ["A", "B"] });
    expect(genres.seasonLanguage).toBeUndefined();
    expect(aired).toMatchObject({ seasonLanguage: "ja" });
    expect(aired.enumOptions).toBeUndefined();
  });

  it("keeps externalPriority deduped/lowercased when no catalog is available", () => {
    const cleaned = cleanedVault(vault({ types: [type({ externalPriority: ["Bangumi", "bangumi", "igdb"] })] }));
    expect(cleaned.types[0].externalPriority).toEqual(["bangumi", "igdb"]);
  });

  // The "preserve unknown values" invariant: when the provider catalog is
  // unavailable (query failed/loading), provider config the user never touched
  // must not be silently dropped.
  describe("preserving provider mappings vs. the catalog", () => {
    const config = vault({
      types: [
        type({
          fields: [
            field({
              field: "name",
              fieldType: "title",
              externalFields: [{ source: "bangumi", field: "name" }],
            }),
          ],
        }),
      ],
    });

    it("keeps the mapping as-is when no catalog is provided", () => {
      const cleaned = cleanedVault(config);
      expect(cleaned.types[0].fields[0].externalFields).toEqual([{ source: "bangumi", field: "name" }]);
    });

    it("keeps a mapping the catalog recognizes", () => {
      const cleaned = cleanedVault(config, catalogWith("bangumi"));
      expect(cleaned.types[0].fields[0].externalFields).toEqual([{ source: "bangumi", field: "name" }]);
    });

    it("drops a mapping the catalog does not recognize", () => {
      const cleaned = cleanedVault(config, catalogWith("igdb"));
      expect(cleaned.types[0].fields[0].externalFields).toBeUndefined();
    });
  });
});
