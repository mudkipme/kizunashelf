import { Trans } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { ExternalLinkIcon } from "lucide-react";

import { configQuery } from "@/api/queries";
import { Badge } from "@/components/ui/badge";
import { RatingScore, ratingNumber } from "@/components/ui/rating-score";
import { configFields, fieldsByType, fieldLabelForKey, type FieldType } from "@/lib/type-config";
import { cn } from "@/lib/utils";
import type { Entity, Relation, TypeConfig } from "@/types/api";

type FrontmatterValue = null | boolean | number | string | FrontmatterValue[] | FrontmatterObject;
type FrontmatterObject = { [key: string]: FrontmatterValue | undefined };

/// The frontmatter entries "Details" would show, after hiding titles/status/dates
/// /refs/relations and values already displayed elsewhere. Exposed so the parent
/// can hide the whole section (title included) when there's nothing to show.
export function useVisibleFrontmatterEntries(
  entity: Entity,
  relationGroups: Array<{ field: string; items: Relation[] }>,
  typeConfig?: TypeConfig,
) {
  const tagsFieldName = useQuery(configQuery()).data?.tagsField ?? undefined;
  return visibleFrontmatterEntries(entity, relationGroups, typeConfig, tagsFieldName);
}

export function FrontmatterPanel({
  entity,
  relationGroups,
  typeConfig,
}: {
  entity: Entity;
  relationGroups: Array<{ field: string; items: Relation[] }>;
  typeConfig?: TypeConfig;
}) {
  const entries = useVisibleFrontmatterEntries(entity, relationGroups, typeConfig);
  const fieldTypes = new Map(
    configFields(typeConfig).map((field) => [field.field, field.fieldType]),
  );

  // The parent gates this section on the same entries, so empty shouldn't reach
  // here — return nothing rather than an empty shell if it ever does.
  if (entries.length === 0) return null;

  // No box, no row rules: a label column of one weight against a value column of
  // another is the whole structure an inspector needs, and it is what macOS's
  // Get Info and Xcode's inspectors do. Alignment across rows comes from the
  // shared grid — rows go `display: contents` at `sm` so every label sits on the
  // same track; below `sm` each row is its own stacked block instead.
  return (
    <dl className="grid gap-y-3 sm:grid-cols-[minmax(8rem,13rem)_minmax(0,1fr)] sm:gap-x-6 sm:gap-y-2">
      {entries.map(([key, value]) => (
        <div key={key} className="grid min-w-0 gap-y-0.5 sm:contents">
          <dt className="min-w-0 truncate text-xs font-medium text-muted-foreground">
            {fieldLabelForKey(typeConfig, key)}
          </dt>
          <dd className="min-w-0 text-sm">
            <FrontmatterValueView value={value} depth={0} fieldType={fieldTypes.get(key)} />
          </dd>
        </div>
      ))}
    </dl>
  );
}

function visibleFrontmatterEntries(
  entity: Entity,
  relationGroups: Array<{ field: string; items: Relation[] }>,
  typeConfig: TypeConfig | undefined,
  tagsFieldName: string | undefined,
): Array<[string, FrontmatterValue | undefined]> {
  const hiddenKeys = new Set<string>([
    // When the opt-in tags feature is enabled, tags have their own display next
    // to the type chip; never in "Details". Disabled → no key to hide.
    ...(tagsFieldName ? [tagsFieldName] : []),
    ...fieldsByType(typeConfig, "title").map((field) => field.field),
    // The status field shows as a badge in the header — not repeated in "Details".
    ...configFields(typeConfig)
      .filter((field) => field.enumRole === "status")
      .map((field) => field.field),
    ...(entity.ratings ?? [])
      .filter((rating) => rating.value != null)
      .map((rating) => rating.field),
    ...entity.dates.map((date) => date.field),
    ...Object.keys(entity.externalRefs),
    ...relationGroups.map((group) => group.field),
  ]);
  const displayedValues = new Set(
    [
      entity.title,
      entity.image,
      ...Object.values(entity.titles),
      ...Object.values(entity.externalRefs),
      ...entity.dates.map((date) => date.value),
    ]
      .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
      .map(normalizeComparable),
  );

  return Object.entries(entity.frontmatter as FrontmatterObject).filter(([key, value]) => {
    if (hiddenKeys.has(key)) return false;
    if (isEmptyTopLevelValue(value)) return false;

    const normalizedValue = normalizedPrimitiveSummary(value);
    if (normalizedValue && displayedValues.has(normalizedValue)) return false;

    return true;
  });
}

function isEmptyTopLevelValue(value: FrontmatterValue | undefined) {
  if (value === null || value === undefined) return true;
  if (typeof value === "string") return value.trim().length === 0;
  if (Array.isArray(value)) return value.length === 0;
  if (typeof value === "object") return Object.keys(value).length === 0;
  return false;
}

function FrontmatterValueView({
  value,
  depth,
  fieldType,
}: {
  value: FrontmatterValue | undefined;
  depth: number;
  fieldType?: FieldType;
}) {
  if (value === undefined || value === null) {
    return (
      <span className="text-muted-foreground">
        <Trans>Empty</Trans>
      </span>
    );
  }

  if (typeof value === "boolean") {
    return (
      <Badge variant={value ? "secondary" : "outline"}>
        {value ? <Trans>Yes</Trans> : <Trans>No</Trans>}
      </Badge>
    );
  }

  if (typeof value === "number") {
    if (fieldType === "rating") return <RatingScore value={value} />;
    return <span className="tabular-nums">{value}</span>;
  }

  if (typeof value === "string") {
    if (fieldType === "rating" && ratingNumber(value) !== undefined)
      return <RatingScore value={value} />;
    return <StringValue value={value} />;
  }

  if (Array.isArray(value)) {
    if (value.length === 0)
      return (
        <span className="text-muted-foreground">
          <Trans>Empty list</Trans>
        </span>
      );

    const primitiveItems = value.every(isPrimitiveMetadataValue);
    if (primitiveItems) {
      return (
        <div className="flex min-w-0 flex-wrap gap-1">
          {value.map((item, index) => (
            <Badge
              key={index}
              variant="outline"
              className="max-w-full break-words whitespace-normal"
            >
              <PrimitiveInlineValue value={item} />
            </Badge>
          ))}
        </div>
      );
    }

    // A fill, not a stroke: each item needs to read as one unit, and a tint does
    // that without adding four more lines to a page that already nests.
    return (
      <div className="flex min-w-0 flex-col gap-2">
        {value.map((item, index) => (
          <div key={index} className="min-w-0 rounded-md bg-muted/60 px-2.5 py-2">
            <div className="mb-1 text-xs font-medium text-muted-foreground uppercase">
              <Trans>Item {index + 1}</Trans>
            </div>
            <FrontmatterValueView value={item} depth={depth + 1} />
          </div>
        ))}
      </div>
    );
  }

  const entries = Object.entries(value).filter(([, item]) => item !== undefined);
  if (entries.length === 0)
    return (
      <span className="text-muted-foreground">
        <Trans>Empty object</Trans>
      </span>
    );

  // Nesting is shown with a single rule down the left and an indent — one line
  // instead of a box's four, and the one that actually encodes "this belongs to
  // the row above". A top-level object skips it *where the label column already
  // says the same thing*, which is only from `sm` up; stacked on mobile there is
  // no column to lean on, so the rule comes back.
  return (
    <dl
      className={cn(
        "grid min-w-0 gap-y-1.5 sm:grid-cols-[minmax(6rem,10rem)_minmax(0,1fr)] sm:gap-x-4 sm:gap-y-1",
        depth > 0 ? "border-l pl-3" : "max-sm:border-l max-sm:pl-3",
      )}
    >
      {entries.map(([key, item]) => (
        <div key={key} className="grid min-w-0 gap-y-0.5 sm:contents">
          <dt className="min-w-0 truncate text-xs text-muted-foreground">{formatKey(key)}</dt>
          <dd className="min-w-0">
            <FrontmatterValueView value={item} depth={depth + 1} />
          </dd>
        </div>
      ))}
    </dl>
  );
}

function StringValue({ value }: { value: string }) {
  const normalized = value.trim();
  if (!normalized)
    return (
      <span className="text-muted-foreground">
        <Trans>Empty</Trans>
      </span>
    );

  if (isUrl(normalized)) {
    return (
      <a
        href={normalized}
        target="_blank"
        rel="noreferrer"
        className="inline-flex max-w-full items-center gap-1 truncate text-primary hover:underline"
      >
        <span className="truncate">{normalized}</span>
        <ExternalLinkIcon />
      </a>
    );
  }

  return <span className="break-words">{normalized}</span>;
}

function PrimitiveInlineValue({ value }: { value: FrontmatterValue | undefined }) {
  if (value === null || value === undefined) return <Trans>Empty</Trans>;
  if (typeof value === "boolean") return value ? <Trans>Yes</Trans> : <Trans>No</Trans>;
  return <>{String(value)}</>;
}

function isPrimitiveMetadataValue(value: FrontmatterValue | undefined) {
  return value === null || ["boolean", "number", "string", "undefined"].includes(typeof value);
}

function normalizedPrimitiveSummary(value: FrontmatterValue | undefined): string | undefined {
  if (value === null || value === undefined) return undefined;
  if (Array.isArray(value)) {
    const values: string[] = value
      .map(normalizedPrimitiveSummary)
      .filter((item): item is string => typeof item === "string");
    return values.length > 0 ? values.join(", ") : undefined;
  }
  if (typeof value === "object") return undefined;
  return normalizeComparable(String(value));
}

function normalizeComparable(value: string) {
  return stripWikilink(value).trim().toLowerCase();
}

function stripWikilink(value: string) {
  const match = value.match(/^\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|([^\]]+))?\]\]$/);
  return match?.[2] ?? match?.[1] ?? value;
}

function formatKey(key: string) {
  return key.replace(/[_-]+/g, " ");
}

function isUrl(value: string) {
  return /^https?:\/\//i.test(value);
}
