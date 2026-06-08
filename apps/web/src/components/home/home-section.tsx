import { ArrowRightIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { HomeEntityCard } from "@/components/home/home-entity-card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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
      <header className="flex min-h-12 items-center gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <h2 className="truncate text-sm font-semibold">{section.title}</h2>
            <Badge variant="secondary">{section.total}</Badge>
          </div>
          <div className="mt-1 truncate text-xs text-muted-foreground">{section.typeLabel}</div>
        </div>
        <Button asChild variant="ghost" size="sm">
          <Link to={libraryHref(section)}>
            View
            <ArrowRightIcon />
          </Link>
        </Button>
      </header>

      <div className="grid grid-cols-[repeat(auto-fill,minmax(144px,1fr))] gap-3 sm:grid-cols-[repeat(auto-fill,minmax(156px,1fr))] xl:grid-cols-[repeat(auto-fill,minmax(168px,1fr))]">
        {section.items.map((entity) => (
          <HomeEntityCard key={entity.id} entity={entity} labelsByType={labelsByType} />
        ))}
        {section.items.length === 0 ? (
          <div className="col-span-full rounded-md border px-3 py-8 text-center text-sm text-muted-foreground">
            No entries
          </div>
        ) : null}
      </div>
    </section>
  );
}

function libraryHref(section: HomeSectionResponse) {
  const params = new URLSearchParams({
    type: section.type,
    sort: section.sort,
    direction: section.direction,
  });
  return `/library?${params}`;
}
