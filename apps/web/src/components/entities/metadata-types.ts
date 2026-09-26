import type { EntitySummary } from "@/types/api";

export type FrontmatterValue =
  | null
  | boolean
  | number
  | string
  | FrontmatterValue[]
  | FrontmatterObject;
export type FrontmatterObject = { [key: string]: FrontmatterValue | undefined };
export type FrontmatterDraft = Record<string, FrontmatterValue>;

export type FieldKind =
  | "text"
  | "select"
  | "number"
  | "boolean"
  | "list"
  | "relation"
  | "season"
  | "date"
  | "image"
  | "imageList";

export type SeasonLanguage = "zh" | "ja" | "en";
export type SeasonKey = "winter" | "spring" | "summer" | "autumn";
export type SeasonRow =
  | { kind: "season"; year: string; season: SeasonKey }
  | { kind: "raw"; value: string };
export type MultiValueOption = { value: string; label?: string; detail?: string };

export type EditableFieldSpec = {
  key: string;
  label: string;
  kind: FieldKind;
  configured: boolean;
  options: string[];
  relationOptions: MultiValueOption[];
  loadRelationOptions?: (query: string, signal: AbortSignal) => Promise<MultiValueOption[]>;
  relationType?: string | null;
  seasonLanguage: SeasonLanguage;
};

export type RelationSuggestionSearch = (params: {
  relationType?: string | null;
  query: string;
  signal: AbortSignal;
}) => Promise<EntitySummary[]>;
