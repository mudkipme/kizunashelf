import { ExternalLinkIcon } from "lucide-react";

import { EmptyLine } from "@/components/assets/detail-section";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import type { Entity, Relation } from "@/types/api";

type FrontmatterValue = null | boolean | number | string | FrontmatterValue[] | FrontmatterObject;
type FrontmatterObject = { [key: string]: FrontmatterValue | undefined };

const titleFieldNames = new Set(["title", "name", "jp_title", "title_ja", "title_en", "title_original"]);

export function FrontmatterPanel({
  entity,
  relationGroups,
}: {
  entity: Entity;
  relationGroups: Array<{ field: string; items: Relation[] }>;
}) {
  const entries = visibleFrontmatterEntries(entity, relationGroups);

  if (entries.length === 0) {
    return <EmptyLine>No additional frontmatter</EmptyLine>;
  }

  return (
    <div className="overflow-hidden rounded-md border">
      <dl className="divide-y">
        {entries.map(([key, value]) => (
          <div
            key={key}
            className="grid min-w-0 gap-2 px-3 py-2 sm:grid-cols-[minmax(8rem,13rem)_minmax(0,1fr)]"
          >
            <dt className="min-w-0 truncate text-xs font-medium text-muted-foreground">{formatKey(key)}</dt>
            <dd className="min-w-0 text-sm">
              <FrontmatterValueView value={value} depth={0} />
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function visibleFrontmatterEntries(
  entity: Entity,
  relationGroups: Array<{ field: string; items: Relation[] }>,
): Array<[string, FrontmatterValue | undefined]> {
  const hiddenKeys = new Set<string>([
    ...entity.dates.map((date) => date.field),
    ...Object.keys(entity.externalRefs),
    ...relationGroups.map((group) => group.field),
  ]);
  const displayedValues = new Set(
    [
      entity.title,
      entity.subtitle,
      entity.status,
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

    if (titleFieldNames.has(key) || key.startsWith("title_")) {
      return !normalizedValue || !displayedValues.has(normalizedValue);
    }

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
}: {
  value: FrontmatterValue | undefined;
  depth: number;
}) {
  if (value === undefined || value === null) {
    return <span className="text-muted-foreground">Empty</span>;
  }

  if (typeof value === "boolean") {
    return <Badge variant={value ? "secondary" : "outline"}>{value ? "Yes" : "No"}</Badge>;
  }

  if (typeof value === "number") {
    return <span className="tabular-nums">{value}</span>;
  }

  if (typeof value === "string") {
    return <StringValue value={value} />;
  }

  if (Array.isArray(value)) {
    if (value.length === 0) return <span className="text-muted-foreground">Empty list</span>;

    const primitiveItems = value.every(isPrimitiveMetadataValue);
    if (primitiveItems) {
      return (
        <div className="flex min-w-0 flex-wrap gap-1">
          {value.map((item, index) => (
            <Badge key={index} variant="outline" className="max-w-full whitespace-normal break-words">
              <PrimitiveInlineValue value={item} />
            </Badge>
          ))}
        </div>
      );
    }

    return (
      <div className="flex min-w-0 flex-col gap-2">
        {value.map((item, index) => (
          <div key={index} className="min-w-0 rounded-md border bg-muted/35 px-2 py-2">
            <div className="mb-1 text-[11px] font-medium uppercase text-muted-foreground">
              Item {index + 1}
            </div>
            <FrontmatterValueView value={item} depth={depth + 1} />
          </div>
        ))}
      </div>
    );
  }

  const entries = Object.entries(value).filter(([, item]) => item !== undefined);
  if (entries.length === 0) return <span className="text-muted-foreground">Empty object</span>;

  return (
    <div className={cn("min-w-0 overflow-hidden rounded-md border", depth > 0 && "bg-background")}>
      <dl className="divide-y">
        {entries.map(([key, item]) => (
          <div
            key={key}
            className="grid min-w-0 gap-1 px-2 py-2 sm:grid-cols-[minmax(7rem,11rem)_minmax(0,1fr)]"
          >
            <dt className="min-w-0 truncate text-xs text-muted-foreground">{formatKey(key)}</dt>
            <dd className="min-w-0">
              <FrontmatterValueView value={item} depth={depth + 1} />
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function StringValue({ value }: { value: string }) {
  const normalized = value.trim();
  if (!normalized) return <span className="text-muted-foreground">Empty</span>;

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
  if (value === null || value === undefined) return <>Empty</>;
  if (typeof value === "boolean") return <>{value ? "Yes" : "No"}</>;
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
