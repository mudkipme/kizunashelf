import { useNumberFormat } from "@/lib/locale";

export function StatTile({ label, value, hint }: { label: string; value: number; hint?: string }) {
  const formatNumber = useNumberFormat();
  return (
    <div className="rounded-md border p-3">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className="mt-1 text-2xl font-semibold tabular-nums">{formatNumber(value)}</div>
      {hint ? <div className="mt-1 text-xs text-muted-foreground">{hint}</div> : null}
    </div>
  );
}
