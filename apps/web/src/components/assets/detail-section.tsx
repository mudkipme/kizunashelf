import type { ReactNode } from "react";

/**
 * One block of the entity detail page.
 *
 * Sections are separated by space alone — no rule, no box. The page spends its
 * whole border budget on the two structural boundaries (under the toolbar,
 * beside the inspector); everything below that is set apart by the gap above it
 * and by the small uppercase label, which is how a native inspector groups
 * things. A leading icon per heading was the other half of the "card" look and
 * carried no information the label didn't.
 */
export function DetailSection({
  title,
  action,
  children,
}: {
  title: string;
  /// Optional control shown on the right of the title row (e.g. a section action).
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="mb-8 flex flex-col gap-3 last:mb-0">
      <div className="flex min-h-(--control-height-sm) items-center justify-between gap-2">
        <h2 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
          {title}
        </h2>
        {action}
      </div>
      {children}
    </section>
  );
}
