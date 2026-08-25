//! The "add entities to this list" picker: a debounced entity search whose
//! results can be added one at a time without closing the dialog.

import { useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { CheckIcon, PlusIcon } from "lucide-react";

import { entitiesQuery } from "@/api/queries";
import { EntityCover } from "@/components/assets/entity-cover";
import { EntityTitle } from "@/components/entities/entity-title";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useTitleLanguage } from "@/lib/language";

export function AddItemsDialog({
  open,
  onOpenChange,
  existingIds,
  disabled,
  onAdd,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  existingIds: Set<string>;
  disabled: boolean;
  onAdd: (entityId: string) => Promise<void>;
}) {
  const { t } = useLingui();
  const language = useTitleLanguage();
  const [query, setQuery] = useState("");
  const [pendingId, setPendingId] = useState<string>();
  const search = useQuery({
    ...entitiesQuery({ q: query.trim() || undefined, pageSize: 20, titleLanguage: language }),
    enabled: open,
  });
  const results = search.data?.items ?? [];

  async function add(entityId: string) {
    setPendingId(entityId);
    try {
      await onAdd(entityId);
    } finally {
      setPendingId(undefined);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent aria-describedby={undefined}>
        <DialogHeader>
          <DialogTitle>
            <Trans>Add items</Trans>
          </DialogTitle>
        </DialogHeader>
        <Input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t`Search entities…`}
          autoFocus
        />
        <div className="flex min-h-0 flex-col gap-1 overflow-auto max-sm:flex-1 sm:max-h-80">
          {search.isPending ? (
            <p className="p-3 text-center text-sm text-muted-foreground">
              <Trans>Loading…</Trans>
            </p>
          ) : results.length === 0 ? (
            <p className="p-3 text-center text-sm text-muted-foreground">
              <Trans>No matching entities.</Trans>
            </p>
          ) : (
            results.map((entity) => {
              const added = existingIds.has(entity.id);
              return (
                <div key={entity.id} className="flex items-center gap-2 rounded-md p-1">
                  <EntityCover entity={entity} />
                  <span className="min-w-0 flex-1">
                    <EntityTitle
                      as="span"
                      entity={entity}
                      language={language}
                      className="block truncate text-sm font-medium"
                    />
                    <span className="block truncate text-xs text-muted-foreground">{entity.typeLabel}</span>
                  </span>
                  {added ? (
                    <Badge variant="outline">
                      <Trans>Added</Trans>
                    </Badge>
                  ) : (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={disabled || pendingId === entity.id}
                      onClick={() => void add(entity.id)}
                    >
                      <PlusIcon data-icon="inline-start" />
                      <Trans>Add</Trans>
                    </Button>
                  )}
                </div>
              );
            })
          )}
        </div>
        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
            <CheckIcon data-icon="inline-start" />
            <Trans>Done</Trans>
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
