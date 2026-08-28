import { cn } from "@/lib/utils";

/**
 * The measure a full-bleed page's content column is held to, and the fact that
 * it centres.
 *
 * A pane on a large window is far wider than anything reads well at — prose
 * worst of all, but a label/value row whose two halves end up a hand apart is no
 * better. `3xl` (48rem) is wide enough for a two-column form or a details table
 * to keep a pair together, and narrow enough that body text doesn't run to an
 * unreadable line. One constant, so no page or section drifts to a width of its
 * own; pages that still use {@link PageContainer} get their width from there.
 */
export const CONTENT_MEASURE = "mx-auto w-full max-w-3xl";

const WIDTHS = {
  standard: "max-w-5xl",
  wide: "max-w-7xl",
  full: "",
} as const;

export type PageWidth = keyof typeof WIDTHS;

export function PageContainer({
  width = "standard",
  className,
  children,
}: {
  width?: PageWidth;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <div className={cn("mx-auto flex w-full flex-col gap-4 p-4", WIDTHS[width], className)}>
      {children}
    </div>
  );
}
