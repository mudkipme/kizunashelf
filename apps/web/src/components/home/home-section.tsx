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
          Nothing here yet
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
  for (const filter of section.filters ?? []) {
    const field = filter.field.trim();
    if (!field) continue;
    for (const value of filter.values ?? []) {
      if (value.trim()) params.append(`filter:${field}`, value);
    }
  }
  return `/library?${params}`;
}
