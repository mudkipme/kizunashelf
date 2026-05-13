import { ArrowRightIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { HomeEntityRow } from "@/components/home/home-entity-row";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { HomeSectionResponse } from "@/types/api";

export function HomeSection({ section }: { section: HomeSectionResponse }) {
  return (
    <section className="min-w-0 border-b lg:border-b-0 lg:border-r lg:last:border-r-0">
      <header className="flex min-h-14 items-center gap-3 border-b px-3 py-2">
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <h2 className="truncate text-sm font-semibold">{section.title}</h2>
            <Badge variant="secondary">{section.total}</Badge>
          </div>
          <div className="mt-1 truncate text-xs text-muted-foreground">
            {section.typeLabel}
            {section.status.length > 0 ? ` · ${section.status.join(", ")}` : ""}
          </div>
        </div>
        <Button asChild variant="ghost" size="sm">
          <Link to={libraryHref(section)}>
            View
            <ArrowRightIcon />
          </Link>
        </Button>
      </header>

      <div>
        {section.items.map((entity) => (
          <HomeEntityRow key={entity.id} entity={entity} />
        ))}
        {section.items.length === 0 ? (
          <div className="px-3 py-8 text-center text-sm text-muted-foreground">No entries</div>
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
  if (section.status.length === 1) params.set("status", section.status[0]);
  return `/library?${params}`;
}
