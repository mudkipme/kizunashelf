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
  externalRefProviderPriority,
  joinPath,
  normalizeVaultConfig,
  relativeToBase,
} from "./settings-model";

describe("arrayEditor", () => {
  it("edits the list without mutating its input", () => {
    const values = [1, 2, 3];
    const onChange = vi.fn();
    const editor = arrayEditor(values, onChange);
    editor.append(4);
    editor.update(1, 9);
    editor.remove(0);
    expect(onChange.mock.calls.map(([next]) => next)).toEqual([
      [1, 2, 3, 4],
      [1, 9, 3],
      [2, 3],
    ]);
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

  it("preserves disabled (null) daily notes for an existing config", () => {
    const result = normalizeVaultConfig(vault({ taxonomyRoot: "T", dailyNotes: null, types: [] }));
    expect(result.dailyNotes).toBeNull();
    expect(result.types).toEqual([]);
    expect(result.taxonomyRoot).toBe("T");
  });

  it("keeps tags disabled (null) unless the config carries a tags block", () => {
    expect(normalizeVaultConfig(vault({ types: [] })).tags).toBeNull();
    expect(normalizeVaultConfig(vault({ tags: { field: "labels" }, types: [] })).tags).toEqual({
      field: "labels",
    });
  });

  it("fills field-level defaults (seasonLanguage, null roles, empty lists)", () => {
    const result = normalizeVaultConfig(
      vault({ types: [type({ fields: [field({ field: "title", fieldType: "title" })] })] }),
    );
    const [entityType] = result.types;
    expect(entityType).toMatchObject({
      icon: "",
      externalPriority: [],
      filename: null,
      bodySections: [],
    });
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
    const cleaned = cleanedVault(vault({ assetRoot: "", dailyNotes: null, types: [] }));
    expect(cleaned.assetRoot).toBeUndefined();
    expect(cleaned.dailyNotes).toBeUndefined();
    expect(cleaned.types).toEqual([]);
  });

  it("saves a tags block only with a non-blank key (trimmed)", () => {
    expect(cleanedVault(vault({ tags: { field: "  labels " }, types: [] })).tags).toEqual({
      field: "labels",
    });
    // A blank key means disabled in the core, so it saves as no block at all.
    expect(cleanedVault(vault({ tags: { field: "  " }, types: [] })).tags).toBeUndefined();
    expect(cleanedVault(vault({ tags: null, types: [] })).tags).toBeUndefined();
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

  it("derives externalPriority from externalRef fields when no catalog is available", () => {
    const cleaned = cleanedVault(
      vault({
        types: [
          type({
            externalPriority: ["igdb", "stale", "igdb"],
            fields: [
              field({ field: "bangumi", fieldType: "externalRef", externalRef: "bangumi" }),
              field({ field: "igdb", fieldType: "externalRef", externalRef: "igdb" }),
              field({ field: "igdb_alt", fieldType: "externalRef", externalRef: "igdb" }),
            ],
          }),
        ],
      }),
    );
    expect(cleaned.types[0].externalPriority).toEqual(["igdb", "bangumi"]);
  });

  it("orders configured externalRef providers and appends missing ones", () => {
    const fields = [
      field({ field: "mal", fieldType: "externalRef", externalRef: "myanimelist" }),
      field({ field: "bgm", fieldType: "externalRef", externalRef: "bangumi" }),
      field({ field: "mal_alt", fieldType: "externalRef", externalRef: "myanimelist" }),
    ];
    expect(externalRefProviderPriority(fields, ["bangumi", "stale"])).toEqual([
      "bangumi",
      "myanimelist",
    ]);
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
      expect(cleaned.types[0].fields[0].externalFields).toEqual([
        { source: "bangumi", field: "name" },
      ]);
    });

    it("keeps a mapping the catalog recognizes", () => {
      const cleaned = cleanedVault(config, catalogWith("bangumi"));
      expect(cleaned.types[0].fields[0].externalFields).toEqual([
        { source: "bangumi", field: "name" },
      ]);
    });

    it("drops a mapping the catalog does not recognize", () => {
      const cleaned = cleanedVault(config, catalogWith("igdb"));
      expect(cleaned.types[0].fields[0].externalFields).toBeUndefined();
    });
  });
});

it("preserves declared and unscaled ratings through the schema editor", () => {
  const original = vault({
    types: [
      type({
        fields: [
          field({ field: "legacy", fieldType: "rating" }),
          field({ field: "personal", fieldType: "rating", ratingMax: 5 }),
          field({ field: "custom", fieldType: "rating", ratingMax: 100 }),
        ],
      }),
    ],
  });
  const { vault: cleaned } = cleanVaultConfig(normalizeVaultConfig(original));
  expect(cleaned?.types[0].fields?.map((field) => field.ratingMax)).toEqual([undefined, 5, 100]);
});
