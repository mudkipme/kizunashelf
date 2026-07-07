import { useLingui } from "@lingui/react/macro";
import { ArrowRightIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { Button } from "@/components/ui/button";

/// The shared header for Home shelves — the "Coming up" widget and each configured
/// section use it, so their titles, counts, and "View" actions line up even though
/// their bodies (compact agenda rows vs. poster grids) look deliberately different.
export function SectionHeader({
  title,
  count,
  subtitle,
  viewHref,
  viewLabel,
}: {
  title: string;
  count?: number;
  subtitle?: string;
  viewHref?: string;
  viewLabel?: string;
}) {
  const { t } = useLingui();
  return (
    <div className="mb-3 flex min-h-8 items-center gap-x-2">
      <h2 className="truncate text-base font-semibold tracking-tight">{title}</h2>
      {count != null ? (
        <span className="shrink-0 text-sm tabular-nums text-muted-foreground">{count}</span>
      ) : null}
      {subtitle ? <span className="truncate text-xs text-muted-foreground">· {subtitle}</span> : null}
      {viewHref ? (
        <Button
          asChild
          variant="ghost"
          size="sm"
          className="-mr-2 ml-auto shrink-0 text-muted-foreground"
        >
          <Link to={viewHref}>
            {viewLabel ?? t`View`}
            <ArrowRightIcon />
          </Link>
        </Button>
      ) : null}
    </div>
  );
}
