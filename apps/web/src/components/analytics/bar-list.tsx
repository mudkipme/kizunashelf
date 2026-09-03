import { Link } from "react-router-dom";

import { useNumberFormat } from "@/lib/locale";
import { cn } from "@/lib/utils";

export type BarListItem = {
  name: string;
  count: number;
  href?: string;
  meta?: string;
};

export function BarList({ items, max }: { items: BarListItem[]; max?: number }) {
  const formatNumber = useNumberFormat();
  const maxCount = max ?? Math.max(1, ...items.map((item) => item.count));

  return (
    <div className="flex flex-col gap-2">
      {items.map((item) => {
        const width = `${Math.max(2, Math.round((item.count / maxCount) * 100))}%`;
        const content = (
          <>
            <div className="flex min-w-0 items-center justify-between gap-2 text-xs">
              <span className="min-w-0 truncate font-medium">{item.name}</span>
              <span className="shrink-0 text-muted-foreground tabular-nums">
                {formatNumber(item.count)}
              </span>
            </div>
            <div className="mt-1 h-2 rounded-sm bg-muted">
              <div className="h-2 rounded-sm bg-primary" style={{ width }} />
            </div>
            {item.meta ? (
              <div className="mt-1 truncate text-xs text-muted-foreground">{item.meta}</div>
            ) : null}
          </>
        );

        return item.href ? (
          <Link
            key={item.name}
            to={item.href}
            className="block rounded-md px-2 py-1 transition-colors hover:bg-accent"
          >
            {content}
          </Link>
        ) : (
          <div key={item.name} className={cn("rounded-md px-2 py-1")}>
            {content}
          </div>
        );
      })}
    </div>
  );
}
