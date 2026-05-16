import type { AnalyticsCoverageMetric } from "@/types/api";

export function CoverageList({ items }: { items: AnalyticsCoverageMetric[] }) {
  return (
    <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
      {items.map((item) => (
        <div key={item.name} className="rounded-md border p-3">
          <div className="flex items-center justify-between gap-2 text-xs">
            <span className="font-medium">{item.name}</span>
            <span className="tabular-nums text-muted-foreground">{item.percent}%</span>
          </div>
          <div className="mt-2 h-2 rounded-sm bg-muted">
            <div className="h-2 rounded-sm bg-primary" style={{ width: `${item.percent}%` }} />
          </div>
          <div className="mt-2 text-xs text-muted-foreground">
            {item.count.toLocaleString()} present · {item.missing.toLocaleString()} missing
          </div>
        </div>
      ))}
    </div>
  );
}
