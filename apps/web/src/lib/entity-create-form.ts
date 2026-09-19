import {
  basenameValidationError,
  deriveBasenameFromTitle,
  normalizeBasename,
} from "@/lib/basename";
import { type FieldConfig, fieldsByType } from "@/lib/type-config";
import type { TypeConfig } from "@/types/api";

/**
 * The title field a new entity's filename derives from, per the schema
 * (mirroring the core's quick-add `candidate_basename_base`): a field matching
 * the type's `filename.titleLanguage`, else its `filename.titleRole`, else the
 * first title field that has a `titleLanguage` (a filename with no title claim
 * is named after the localized frontmatter title), else the `original`-role
 * title field, else the first title field. `undefined` when the type declares
 * no title field.
 */
export function filenameTitleField(typeConfig: TypeConfig | undefined): FieldConfig | undefined {
  const titleFields = fieldsByType(typeConfig, "title");
  if (!titleFields.length) return undefined;
  const filename = typeConfig?.filename;
  const byLanguage = filename?.titleLanguage
    ? titleFields.find((field) => field.titleLanguage === filename.titleLanguage)
    : undefined;
  const byRole = filename?.titleRole
    ? titleFields.find((field) => field.titleRole === filename.titleRole)
    : undefined;
  const localized = titleFields.find((field) => field.titleLanguage);
  const original = titleFields.find((field) => field.titleRole === "original");
  return byLanguage ?? byRole ?? localized ?? original ?? titleFields[0];
}

export type CreateBasenameResolution = {
  /** The basename the create request would submit ("" when none is available). */
  basename: string;
  /** Where it came from: the filename input, the title field, or nowhere. */
  source: "manual" | "title" | "none";
  /** Validation error for a manual value, when invalid. */
  error?: string;
  /** The title-derived fallback, when the title field has a usable value. */
  derived?: string;
  /** Whether creation may proceed. */
  canCreate: boolean;
};

/**
 * Resolve what filename manual entity creation would use: an explicit filename
 * wins (and must validate), otherwise the schema's filename title field
 * supplies it, otherwise creation is blocked — the caller disables Create and
 * explains, instead of silently doing nothing.
 */
export function resolveCreateBasename({
  typeConfig,
  frontmatter,
  manualBasename,
}: {
  typeConfig: TypeConfig | undefined;
  frontmatter: Record<string, unknown>;
  manualBasename: string;
}): CreateBasenameResolution {
  const titleField = filenameTitleField(typeConfig);
  const rawTitle = titleField ? frontmatter[titleField.field] : undefined;
  const derived = typeof rawTitle === "string" ? deriveBasenameFromTitle(rawTitle) : undefined;
  const manual = normalizeBasename(manualBasename);
  if (manual) {
    const error = basenameValidationError(manual);
    return { basename: manual, source: "manual", error, derived, canCreate: !error };
  }
  if (derived) {
    return { basename: derived, source: "title", derived, canCreate: true };
  }
  return { basename: "", source: "none", canCreate: false };
}
