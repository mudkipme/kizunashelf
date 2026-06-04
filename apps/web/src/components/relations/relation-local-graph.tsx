import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/badge";
import { defaultTitleLanguage } from "@/lib/constants";
import { entityTitle } from "@/lib/title-language";
import type { EntitySummary } from "@/types/api";

type GraphNode = {
  entity: EntitySummary;
  x: number;
  y: number;
};

const maxGraphNodes = 14;

export function RelationLocalGraph({
  target,
  sources,
}: {
  target: { targetTitle: string; targetTypeLabel?: string | null; count: number };
  sources: EntitySummary[];
}) {
  const nodes = graphNodes(sources.slice(0, maxGraphNodes));

  return (
    <section className="overflow-auto rounded-md border">
      <div className="min-w-[860px]">
        <header className="flex items-center gap-2 border-b px-3 py-2">
          <h2 className="text-sm font-semibold">Local Graph</h2>
          <Badge variant="secondary">{sources.length} sources</Badge>
          {sources.length > nodes.length ? (
            <Badge variant="outline">showing {nodes.length}</Badge>
          ) : null}
        </header>
        <div className="relative h-[520px]">
          <svg className="absolute inset-0 size-full text-border" aria-hidden="true">
            {nodes.map((node) => (
              <line
                key={node.entity.id}
                x1="50%"
                y1="50%"
                x2={`${node.x}%`}
                y2={`${node.y}%`}
                stroke="currentColor"
                strokeWidth="1"
              />
            ))}
          </svg>
          <div className="absolute left-1/2 top-1/2 flex w-56 -translate-x-1/2 -translate-y-1/2 flex-col items-center gap-1 rounded-md border bg-background px-3 py-2 text-center shadow-xs">
            <span className="line-clamp-2 text-sm font-semibold">{target.targetTitle}</span>
            <span className="flex flex-wrap justify-center gap-1">
              {target.targetTypeLabel ? <Badge variant="outline">{target.targetTypeLabel}</Badge> : null}
              <Badge variant="secondary">{target.count} links</Badge>
            </span>
          </div>
          {nodes.map((node) => (
            <Link
              key={node.entity.id}
              to={`/entities/${encodeURIComponent(node.entity.id)}`}
              className="absolute flex w-44 -translate-x-1/2 -translate-y-1/2 flex-col gap-1 rounded-md border bg-background px-2 py-1.5 shadow-xs transition-colors hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
              style={{ left: `${node.x}%`, top: `${node.y}%` }}
            >
              <span className="truncate text-xs font-medium">
                {entityTitle(node.entity, defaultTitleLanguage)}
              </span>
              <span className="flex min-w-0 items-center gap-1">
                <Badge variant="outline" className="max-w-full truncate px-1.5 py-0 text-[11px] font-normal">
                  {node.entity.typeLabel}
                </Badge>
                {node.entity.status ? (
                  <Badge variant="secondary" className="max-w-full truncate px-1.5 py-0 text-[11px] font-normal">
                    {node.entity.status}
                  </Badge>
                ) : null}
              </span>
            </Link>
          ))}
        </div>
      </div>
    </section>
  );
}

function graphNodes(entities: EntitySummary[]) {
  if (entities.length > 8) {
    const sideCount = Math.ceil(entities.length / 2);
    return entities.map<GraphNode>((entity, index) => {
      const sideIndex = Math.floor(index / 2);
      const y = 12 + (sideIndex / Math.max(1, sideCount - 1)) * 76;
      return {
        entity,
        x: index % 2 === 0 ? 22 : 78,
        y,
      };
    });
  }

  return entities.map<GraphNode>((entity, index, items) => {
    const angle = -Math.PI / 2 + (index / Math.max(1, items.length)) * Math.PI * 2;
    return {
      entity,
      x: 50 + Math.cos(angle) * 36,
      y: 50 + Math.sin(angle) * 34,
    };
  });
}
