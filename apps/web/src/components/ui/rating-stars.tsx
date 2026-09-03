import { StarIcon } from "lucide-react";

import { cn } from "@/lib/utils";

export function RatingStars({ value, className }: { value: unknown; className?: string }) {
  const rating = ratingNumber(value);
  if (rating === undefined) return null;

  const max = rating > 5 ? 10 : 5;
  const filled = Math.round(Math.max(0, Math.min(max, rating)));
  const label = `${formatRating(rating)} out of ${max}`;

  return (
    <span
      className={cn("inline-flex items-center gap-0.5 align-middle", className)}
      aria-label={label}
      title={label}
    >
      {Array.from({ length: max }, (_, index) => (
        <StarIcon
          key={index}
          className={cn(
            "size-3.5",
            index < filled ? "fill-primary text-primary" : "text-muted-foreground opacity-35",
          )}
          aria-hidden="true"
        />
      ))}
    </span>
  );
}

export function ratingNumber(value: unknown): number | undefined {
  const number =
    typeof value === "number" ? value : typeof value === "string" ? Number(value.trim()) : NaN;
  return Number.isFinite(number) ? number : undefined;
}

function formatRating(value: number) {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}
