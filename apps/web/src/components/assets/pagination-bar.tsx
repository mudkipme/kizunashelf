import { Trans, useLingui } from "@lingui/react/macro";
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { visiblePages } from "@/lib/pagination";

export function PaginationBar({
  page,
  totalPages,
  total,
  pageSize,
  onPageChange,
}: {
  page: number;
  totalPages: number;
  total: number;
  pageSize: number;
  onPageChange: (page: number) => void;
}) {
  const { t } = useLingui();
  if (totalPages <= 1) return null;

  const start = total === 0 ? 0 : (page - 1) * pageSize + 1;
  const end = Math.min(total, page * pageSize);
  const pages = visiblePages(page, totalPages);

  return (
    <div className="flex flex-wrap items-center justify-between gap-2 border-t px-3 pt-2 pb-[calc(0.5rem+env(safe-area-inset-bottom))] text-xs text-muted-foreground">
      <span>
        <Trans>
          {start}–{end} of {total}
        </Trans>
      </span>
      <nav className="flex items-center gap-1" aria-label={t`Pagination`}>
        <Button
          variant="outline"
          size="sm"
          onClick={() => onPageChange(page - 1)}
          disabled={page <= 1}
        >
          <ChevronLeftIcon data-icon="inline-start" />
          <Trans>Previous</Trans>
        </Button>
        {pages.map((item, index) =>
          item === "ellipsis" ? (
            <span key={`${item}-${index}`} className="px-2">
              ...
            </span>
          ) : (
            <Button
              key={item}
              aria-label={t`Page ${item}`}
              aria-current={item === page ? "page" : undefined}
              variant={item === page ? "secondary" : "ghost"}
              size="sm"
              onClick={() => onPageChange(item)}
            >
              {item}
            </Button>
          ),
        )}
        <Button
          variant="outline"
          size="sm"
          onClick={() => onPageChange(page + 1)}
          disabled={page >= totalPages}
        >
          <Trans>Next</Trans>
          <ChevronRightIcon data-icon="inline-end" />
        </Button>
      </nav>
    </div>
  );
}
