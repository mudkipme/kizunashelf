import { StarIcon } from "lucide-react";

import { cn } from "@/lib/utils";

/** A score is never rounded or used to guess its scale. */
export function RatingScore({
  value,
  max,
  className,
}: {
  value: unknown;
  max?: number | null;
  className?: string;
}) {
  const rating = ratingNumber(value);
  if (rating === undefined) return null;
  return (
    <span className={cn("inline-flex items-center gap-1 tabular-nums", className)}>
      <StarIcon className="size-3.5 text-primary" aria-hidden="true" />
      <span>
        {rating}
        {max != null ? ` / ${max}` : ""}
      </span>
    </span>
  );
}

export function ratingNumber(value: unknown): number | undefined {
  const number =
    typeof value === "number"
      ? value
      : typeof value === "string" && value.trim()
        ? Number(value.trim())
        : NaN;
  return Number.isFinite(number) ? number : undefined;
}
