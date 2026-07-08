import { useMemo } from "react";
import { msg } from "@lingui/core/macro";
import { Plural, Trans, useLingui } from "@lingui/react/macro";
import type { MessageDescriptor } from "@lingui/core";
import { useQuery } from "@tanstack/react-query";
import { LayoutGridIcon, ListIcon, SparklesIcon, TriangleAlertIcon } from "lucide-react";
import { useParams, useSearchParams } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { configQuery, smartListQuery, smartListResultsQuery } from "@/api/queries";
import { EntityGridItem } from "@/components/assets/entity-grid-item";
import { EntityListItem } from "@/components/assets/entity-list-item";
import { PaginationBar } from "@/components/assets/pagination-bar";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Placeholder } from "@/components/ui/placeholder";
import { pageSize } from "@/lib/constants";
import { useTitleLanguage } from "@/lib/language";
import { fieldLabelsByType, typeHasCoverField } from "@/lib/type-config";
import type { SmartFilterGroup, SmartFilterRule, SmartListDetail } from "@/types/api";

export function SmartListPage() {
  const { id = "" } = useParams();
  const [searchParams, setSearchParams] = useSearchParams();
  const language = useTitleLanguage();
  const config = useQuery(configQuery());

  const detail = useQuery(smartListQuery(id));
  const data = detail.data;

  // The active view (tab) rides on `?view=`; the file's first supported view
  // is the default — same as what the server evaluates when `view` is omitted.
  const viewParam = searchParams.get("view") ?? undefined;
  const activeView =
    (viewParam ? data?.views.find((view) => view.name === viewParam) : undefined) ??
    data?.views[0];
  const page = Math.max(1, Number(searchParams.get("page")) || 1);

  const results = useQuery(
    smartListResultsQuery(id, {
      view: activeView?.name,
      page,
      pageSize,
      titleLanguage: language,
    }),
  );

  const fieldLabels = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);
  const scopeConfig = data?.scope
    ? config.data?.types.find((type) => type.id === data.scope)
    : undefined;
  // Without a type scope the results span all types, so keep covers and type
  // badges on; scoped lists follow the type's own schema, like the library.
  const showCover = !data?.scope || typeHasCoverField(scopeConfig);
  const showType = !data?.scope;

  const entities = results.data?.items ?? [];
  const total = results.data?.total ?? 0;
  const totalPages = results.data?.totalPages ?? 1;

  const setParam = (key: string, value: string | undefined) => {
    setSearchParams(
      (params) => {
        const next = new URLSearchParams(params);
        if (value === undefined) next.delete(key);
        else next.set(key, value);
        return next;
      },
      { replace: true },
    );
  };

  return (
    <AppFrame
      error={
        detail.error
          ? errorMessage(detail.error)
          : results.error
            ? errorMessage(results.error)
            : undefined
      }
    >
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4 p-4">
        {detail.isPending ? (
          <Placeholder>
            <Trans>Loading…</Trans>
          </Placeholder>
        ) : !data ? (
          <Placeholder>
            <Trans>Smart list not found</Trans>
          </Placeholder>
        ) : (
          <>
            <header className="flex flex-wrap items-center gap-2">
              <SparklesIcon className="size-5 shrink-0 text-muted-foreground" />
              <h1 className="truncate text-lg font-semibold">{data.name}</h1>
              {scopeConfig ? <Badge variant="outline">{scopeConfig.label}</Badge> : null}
              <span className="mr-auto text-xs text-muted-foreground">
                <Plural value={total} one="# match" other="# matches" />
              </span>
              {data.views.length > 0 ? (
                <div className="flex items-center gap-1 rounded-md border p-0.5">
                  {data.views.map((view) => (
                    <Button
                      key={view.name}
                      type="button"
                      size="sm"
                      variant={view.name === activeView?.name ? "secondary" : "ghost"}
                      onClick={() => {
                        setParam("page", undefined);
                        setParam("view", view.name);
                      }}
                    >
                      {view.layout === "grid" ? (
                        <LayoutGridIcon data-icon="inline-start" />
                      ) : (
                        <ListIcon data-icon="inline-start" />
                      )}
                      {view.name}
                    </Button>
                  ))}
                </div>
              ) : null}
            </header>

            <CriteriaSummary detail={data} />

            {(data.warnings?.length ?? 0) > 0 ? (
              <WarningsBanner warnings={data.warnings ?? []} />
            ) : null}

            <section className="flex min-h-0 flex-col overflow-hidden rounded-md border">
              <div className="min-h-0 flex-1 overflow-auto">
                {activeView?.layout === "grid" ? (
                  <div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3 p-3">
                    {entities.map((entity) => (
                      <EntityGridItem
                        key={entity.id}
                        entity={entity}
                        labelsByType={fieldLabels}
                        showCover={showCover}
                        showType={showType}
                      />
                    ))}
                  </div>
                ) : (
                  entities.map((entity) => (
                    <EntityListItem
                      key={entity.id}
                      entity={entity}
                      labelsByType={fieldLabels}
                      showCover={showCover}
                      showType={showType}
                    />
                  ))
                )}
                {!results.isFetching && entities.length === 0 ? (
                  <div className="p-8 text-center text-sm text-muted-foreground">
                    <Trans>No entries match the criteria</Trans>
                  </div>
                ) : null}
              </div>
              <PaginationBar
                page={results.data?.page ?? page}
                totalPages={totalPages}
                total={total}
                pageSize={pageSize}
                onPageChange={(next) => setParam("page", next > 1 ? String(next) : undefined)}
              />
            </section>
          </>
        )}
      </div>
    </AppFrame>
  );
}

/// The parts of the underlying `.base` file the app ignores (unsupported
/// filters, views, sorts). They still apply in Obsidian and are preserved on
/// every save, so surface them rather than silently diverging.
function WarningsBanner({ warnings }: { warnings: string[] }) {
  return (
    <div className="flex gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-xs">
      <TriangleAlertIcon className="mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-400" />
      <div className="flex flex-col gap-1">
        <p className="font-medium">
          <Trans>Some of this file's syntax isn't supported here and is ignored:</Trans>
        </p>
        <ul className="list-inside list-disc text-muted-foreground">
          {warnings.map((warning) => (
            <li key={warning} className="break-all">
              {warning}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

/// Read-only rendering of the list's criteria (the rule builder arrives with
/// the write path). Rules render as compact chips; nested groups as bordered
/// clusters with their own conjunction.
function CriteriaSummary({ detail }: { detail: SmartListDetail }) {
  const group = detail.filters;
  const rules = group.rules ?? [];
  const empty = rules.length === 0 && (group.groups?.length ?? 0) === 0;
  if (empty) {
    return (
      <p className="text-xs text-muted-foreground">
        {detail.scope ? (
          <Trans>No further criteria — every entry of this type matches.</Trans>
        ) : (
          <Trans>No criteria — everything in the library matches.</Trans>
        )}
      </p>
    );
  }
  return (
    <div className="flex flex-wrap items-center gap-1.5 text-xs">
      <ConjunctionLabel conjunction={group.conjunction} />
      {rules.map((rule, index) => (
        <RuleChip key={index} rule={rule} />
      ))}
      {(group.groups ?? []).map((subgroup, index) => (
        <span
          key={index}
          className="flex flex-wrap items-center gap-1.5 rounded-md border border-dashed px-1.5 py-1"
        >
          <ConjunctionLabel conjunction={subgroup.conjunction} />
          {(subgroup.rules ?? []).map((rule, ruleIndex) => (
            <RuleChip key={ruleIndex} rule={rule} />
          ))}
        </span>
      ))}
    </div>
  );
}

function ConjunctionLabel({ conjunction }: { conjunction: SmartFilterGroup["conjunction"] }) {
  return (
    <span className="font-medium text-muted-foreground">
      {conjunction === "any" ? (
        <Trans comment="Prefix before a group of filter-criteria chips: at least one must match">
          any of
        </Trans>
      ) : conjunction === "none" ? (
        <Trans comment="Prefix before a group of filter-criteria chips: none may match">
          none of
        </Trans>
      ) : (
        <Trans comment="Prefix before a group of filter-criteria chips: all must match">
          all of
        </Trans>
      )}
    </span>
  );
}

function RuleChip({ rule }: { rule: SmartFilterRule }) {
  const { t } = useLingui();
  const label = formatRule(rule, t);
  const ignored = rule.kind === "unsupported";
  return (
    <Badge variant="outline" className={ignored ? "text-muted-foreground line-through" : undefined}>
      {label}
    </Badge>
  );
}

const opSymbols: Record<string, string> = {
  eq: "=",
  ne: "≠",
  gt: ">",
  gte: "≥",
  lt: "<",
  lte: "≤",
};

const unitSymbols: Record<string, string> = {
  days: "d",
  weeks: "w",
  months: "M",
  years: "y",
};

// The connective words of the criteria chips, as lazy descriptors: macros
// don't transform inside plain helper functions, so `formatRule` resolves
// these through the component-scoped `t` instead. All of them join a field
// name and a value into a compact phrase like `genres contains comedy`.
const ruleWords = {
  not: msg({
    comment:
      "Negation prefix in a filter-criteria chip, e.g. 'not genres contains comedy'",
    message: "not",
  }),
  yes: msg({
    comment: "Value of a boolean field in a filter-criteria chip, e.g. 'favorite = yes'",
    message: "yes",
  }),
  no: msg({
    comment: "Value of a boolean field in a filter-criteria chip, e.g. 'favorite = no'",
    message: "no",
  }),
  contains: msg({
    comment:
      "Verb between a field name and values in a filter-criteria chip, e.g. 'genres contains comedy, drama' (any of the values)",
    message: "contains",
  }),
  containsAll: msg({
    comment:
      "Verb between a field name and values in a filter-criteria chip, e.g. 'genres contains all comedy, drama' (every value required)",
    message: "contains all",
  }),
  startsWith: msg({
    comment: "Verb in a filter-criteria chip, e.g. 'title starts with My'",
    message: "starts with",
  }),
  endsWith: msg({
    comment: "Verb in a filter-criteria chip, e.g. 'title ends with !'",
    message: "ends with",
  }),
  hasValue: msg({
    comment: "Predicate after a field name in a filter-criteria chip, e.g. 'rating has a value'",
    message: "has a value",
  }),
  isEmpty: msg({
    comment: "Predicate after a field name in a filter-criteria chip, e.g. 'rating is empty'",
    message: "is empty",
  }),
  linksTo: msg({
    comment:
      "Verb before an entity name in a filter-criteria chip, e.g. 'links to Kyoto Animation'",
    message: "links to",
  }),
  inFolder: msg({
    comment: "Preposition before a folder path in a filter-criteria chip, e.g. 'in Media/Anime'",
    message: "in",
  }),
};

/// One rule as a compact human-readable chip. Field names are user data (never
/// localized); the connective words are. Relative dates use the same compact
/// notation as the file (`today − 90d`), which is language-neutral.
function formatRule(rule: SmartFilterRule, t: (descriptor: MessageDescriptor) => string): string {
  const field = rule.field ?? "";
  const values = rule.values?.join(", ") ?? "";
  const not = rule.negated ? `${t(ruleWords.not)} ` : "";
  switch (rule.kind) {
    case "compare": {
      const op = opSymbols[rule.op ?? "eq"] ?? "=";
      let value: string;
      if (rule.relative) {
        const base = field === "file.mtime" ? "now" : "today";
        const offset = `${rule.relative.amount}${unitSymbols[rule.relative.unit] ?? rule.relative.unit}`;
        value =
          rule.relative.amount === 0
            ? base
            : `${base} ${rule.relative.future ? "+" : "−"} ${offset}`;
      } else if (rule.date) {
        value = rule.date;
      } else if (rule.number !== undefined && rule.number !== null) {
        value = String(rule.number);
      } else if (rule.boolean !== undefined && rule.boolean !== null) {
        value = rule.boolean ? t(ruleWords.yes) : t(ruleWords.no);
      } else {
        value = rule.value ?? "";
      }
      return `${field} ${op} ${value}`;
    }
    case "contains":
      return rule.mode === "all"
        ? `${not}${field} ${t(ruleWords.containsAll)} ${values}`
        : `${not}${field} ${t(ruleWords.contains)} ${values}`;
    case "startsWith":
      return `${not}${field} ${t(ruleWords.startsWith)} ${values}`;
    case "endsWith":
      return `${not}${field} ${t(ruleWords.endsWith)} ${values}`;
    case "isEmpty":
      return rule.negated
        ? `${field} ${t(ruleWords.hasValue)}`
        : `${field} ${t(ruleWords.isEmpty)}`;
    case "hasTag":
      return `${not}${(rule.values ?? []).map((tag) => `#${tag}`).join(" ")}`;
    case "linksTo":
      return `${not}${t(ruleWords.linksTo)} ${values}`;
    case "inFolder":
      return `${not}${t(ruleWords.inFolder)} ${values}`;
    case "unsupported":
      return (rule.raw ?? "").trim().replace(/\s+/g, " ");
    default:
      return values;
  }
}
