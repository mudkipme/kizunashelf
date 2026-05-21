import { z } from "zod";

export const EntityFieldsSchema = z.object({
  title: z.array(z.string()).optional(),
  subtitle: z.array(z.string()).optional(),
  image: z.array(z.string()).optional(),
  status: z.array(z.string()).optional(),
  date: z.array(z.string()).optional(),
  externalRefs: z.array(z.string()).optional(),
  relations: z.array(z.string()).optional(),
});

export const EntityTypeConfigSchema = z.object({
  id: z.string(),
  label: z.string(),
  path: z.string(),
  fields: EntityFieldsSchema,
});
export type EntityTypeConfig = z.infer<typeof EntityTypeConfigSchema>;

export const HomeSectionConfigSchema = z.object({
  id: z.string(),
  title: z.string(),
  type: z.string(),
  status: z.union([z.string(), z.array(z.string())]).optional(),
  limit: z.number().int().positive().optional(),
  sort: z.string().optional(),
  direction: z.enum(["asc", "desc"]).optional(),
});
export type HomeSectionConfig = z.infer<typeof HomeSectionConfigSchema>;

export const HomeConfigSchema = z.object({
  title: z.string().optional(),
  sections: z.array(HomeSectionConfigSchema).optional(),
});
export type HomeConfig = z.infer<typeof HomeConfigSchema>;

export const DailyNotesConfigSchema = z.object({
  paths: z.array(z.string()).optional(),
  datePattern: z.string().optional(),
  snippetMaxLength: z.number().int().positive().optional(),
});
export type DailyNotesConfig = z.infer<typeof DailyNotesConfigSchema>;

export const KizunaConfigSchema = z.object({
  vaultRoot: z.string(),
  taxonomyRoot: z.string(),
  types: z.array(EntityTypeConfigSchema),
  relationshipFields: z.array(z.string()),
  readConcurrency: z.number().int().positive().optional(),
  home: HomeConfigSchema.optional(),
  dailyNotes: DailyNotesConfigSchema.optional(),
});
export type KizunaConfig = z.infer<typeof KizunaConfigSchema>;

export type EntityDateValue = {
  field: string;
  value: string;
};

export type EntitySummary = {
  id: string;
  type: string;
  typeLabel: string;
  title: string;
  subtitle?: string;
  status?: string;
  dates: EntityDateValue[];
  image?: string;
  summary?: string;
  path: string;
  basename: string;
  externalRefs: Record<string, string>;
  relationCount: number;
};

export type Entity = EntitySummary & {
  frontmatter: Record<string, unknown>;
  body: string;
  raw: string;
};

export type Relation = {
  sourceId: string;
  targetId?: string;
  targetTitle: string;
  targetType?: string;
  field: string;
  direction: "out" | "in";
};

export type Library = {
  config: KizunaConfig;
  entities: Entity[];
  summaries: EntitySummary[];
  relations: Relation[];
  generatedAt: string;
};
