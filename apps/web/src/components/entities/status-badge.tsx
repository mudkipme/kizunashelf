import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";
import type { CanonicalStatus, ResolvedStatus } from "@/types/api";

/// Canonical-status tints. The label always carries the meaning (it's the user's
/// own option string), so color only reinforces it — safe for colorblind readers.
/// Tuned to sit quietly against the app's muted surfaces in both themes. An
/// unmapped value (no canonical) falls back to a neutral outline.
const CANONICAL_CLASS: Record<CanonicalStatus, string> = {
  planning:
    "border-transparent bg-sky-100 text-sky-700 dark:bg-sky-950 dark:text-sky-300",
  ongoing:
    "border-transparent bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300",
  paused:
    "border-transparent bg-amber-100 text-amber-700 dark:bg-amber-950 dark:text-amber-300",
  completed:
    "border-transparent bg-violet-100 text-violet-700 dark:bg-violet-950 dark:text-violet-300",
  dropped:
    "border-transparent bg-rose-100 text-rose-700 dark:bg-rose-950 dark:text-rose-300",
};

const DOT_CLASS: Record<CanonicalStatus, string> = {
  planning: "bg-sky-500",
  ongoing: "bg-emerald-500",
  paused: "bg-amber-500",
  completed: "bg-violet-500",
  dropped: "bg-rose-500",
};

/// A badge showing an entity's status — the user's own option label, tinted by the
/// canonical status it maps to. Renders nothing when there is no status value.
export function StatusBadge({
  status,
  className,
}: {
  status: ResolvedStatus | null | undefined;
  className?: string;
}) {
  if (!status) return null;
  const canonical = status.canonical ?? undefined;
  return (
    <Badge
      variant={canonical ? "default" : "outline"}
      className={cn(canonical && CANONICAL_CLASS[canonical], "gap-1.5", className)}
    >
      {canonical ? (
        <span className={cn("size-1.5 rounded-full", DOT_CLASS[canonical])} aria-hidden />
      ) : null}
      {status.value}
    </Badge>
  );
}
