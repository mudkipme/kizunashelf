import { Trans } from "@lingui/react/macro";

import { HomeEntityCard } from "@/components/home/home-entity-card";
import { SectionHeader } from "@/components/home/section-header";
import type { HomeSectionResponse } from "@/types/api";

export function HomeSection({
  section,
  labelsByType,
}: {
  section: HomeSectionResponse;
  labelsByType?: ReadonlyMap<string, ReadonlyMap<string, string>>;
}) {
  return (
    <section className="min-w-0">
      <SectionHeader
        title={section.title}
        count={section.total}
        subtitle={section.typeLabel}
        viewHref={libraryHref(section)}
      />

      {section.items.length ? (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(144px,1fr))] gap-3 sm:grid-cols-[repeat(auto-fill,minmax(156px,1fr))] xl:grid-cols-[repeat(auto-fill,minmax(168px,1fr))]">
          {section.items.map((entity) => (
            <HomeEntityCard key={entity.id} entity={entity} labelsByType={labelsByType} />
          ))}
        </div>
      ) : (
        <div className="rounded-lg border border-dashed px-3 py-10 text-center text-sm text-muted-foreground">
          <Trans>Nothing here yet</Trans>
        </div>
      )}
    </section>
  );
}

function libraryHref(section: HomeSectionResponse) {
  const params = new URLSearchParams({
    type: section.type,
    sort: section.sort,
    direction: section.direction,
  });
  // Best-effort projection of the section's criteria onto the library page's
  // URL filters: equality and membership rules carry over (including an
  // "any of" subgroup of equalities on one field, which the library ORs);
  // richer rules (dates, numbers, negations) have no URL form and are left
  // off — the link then shows a superset of the section.
  const criteria = section.criteria;
  const append = (field: string | null | undefined, values: (string | undefined)[]) => {
    const key = field?.trim();
    if (!key) return;
    for (const value of values) {
      if (value?.trim()) params.append(`filter:${key}`, value);
    }
  };
  if (criteria?.conjunction === "all" || !criteria) {
    for (const rule of criteria?.rules ?? []) {
      if (rule.negated) continue;
      if (rule.kind === "compare" && rule.op === "eq" && rule.value) {
        append(rule.field, [rule.value]);
      } else if (rule.kind === "contains" && rule.mode !== "all") {
        append(rule.field, rule.values ?? []);
      }
    }
    for (const group of criteria?.groups ?? []) {
      if (group.conjunction !== "any") continue;
      const rules = group.rules ?? [];
      const fields = new Set(rules.map((rule) => rule.field));
      const allEq = rules.every(
        (rule) => rule.kind === "compare" && rule.op === "eq" && rule.value && !rule.negated,
      );
      if (fields.size === 1 && allEq) {
        append(rules[0]?.field, rules.map((rule) => rule.value ?? undefined));
      }
    }
  }
  return `/library?${params}`;
}
