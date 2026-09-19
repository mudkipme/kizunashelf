import { Trans } from "@lingui/react/macro";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckIcon, SparklesIcon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { invalidateSmartListData } from "@/api/invalidate-smart-list-data";
import { queryKeys } from "@/api/queries";
import { addSuggestedSmartLists, fetchSmartListSuggestions } from "@/api/smart-lists";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useLanguagePreference } from "@/lib/language";

export function SuggestedListsButton({ disabled }: { disabled?: boolean }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button type="button" variant="outline" disabled={disabled} onClick={() => setOpen(true)}>
        <SparklesIcon data-icon="inline-start" />
        <Trans>Add suggested lists</Trans>
      </Button>
      {open && <SuggestedListsDialog onClose={() => setOpen(false)} />}
    </>
  );
}

function SuggestedListsDialog({ onClose }: { onClose: () => void }) {
  const language = useLanguagePreference();
  const queryClient = useQueryClient();
  const suggestions = useQuery({
    queryKey: queryKeys.smartListSuggestions(language),
    queryFn: ({ signal }) => fetchSmartListSuggestions(language, { signal }),
  });
  const [selected, setSelected] = useState<Set<string> | null>(null);
  const available = suggestions.data?.suggestions.filter((item) => !item.showOnHome) ?? [];
  const selectedIds = available
    .filter((item) => selected === null || selected.has(item.id))
    .map((item) => item.id);
  const create = useMutation({
    mutationFn: () => addSuggestedSmartLists(language, selectedIds),
    onSuccess: async () => {
      await invalidateSmartListData(queryClient);
      onClose();
    },
    onError: (error) => {
      toast.error(errorMessage(error));
      // Creation is atomic per file. Refresh after a partial failure so retry
      // only needs to finish the missing suggestions.
      void invalidateSmartListData(queryClient);
    },
  });
  return (
    <Dialog open onOpenChange={(open) => !open && !create.isPending && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            <Trans>Add suggested lists</Trans>
          </DialogTitle>
          <DialogDescription>
            <Trans>Choose lists to add to Home. Existing lists keep their changes.</Trans>
          </DialogDescription>
        </DialogHeader>
        {suggestions.isPending ? (
          <p>
            <Trans>Loading…</Trans>
          </p>
        ) : suggestions.error ? (
          <Alert>{errorMessage(suggestions.error)}</Alert>
        ) : !suggestions.data?.suggestions.length ? (
          <p className="text-sm text-muted-foreground">
            <Trans>
              No suggestions for your current types. You can create a smart list in Lists.
            </Trans>
          </p>
        ) : (
          <div className="flex max-h-[50vh] flex-col gap-2 overflow-y-auto">
            {suggestions.data.suggestions.map((item) => {
              const checked = item.showOnHome || selectedIds.includes(item.id);
              return (
                <Button
                  key={item.id}
                  type="button"
                  variant={checked ? "secondary" : "outline"}
                  className="justify-start"
                  disabled={item.showOnHome || create.isPending}
                  aria-pressed={checked}
                  onClick={() =>
                    setSelected(() => {
                      const next = new Set(selectedIds);
                      if (checked) next.delete(item.id);
                      else next.add(item.id);
                      return next;
                    })
                  }
                >
                  {checked && <CheckIcon data-icon="inline-start" />}
                  <span className="truncate">{item.name}</span>
                  {item.showOnHome && (
                    <span>
                      {" "}
                      — <Trans>Already on Home</Trans>
                    </span>
                  )}
                </Button>
              );
            })}
          </div>
        )}
        <DialogFooter>
          <Button type="button" variant="outline" disabled={create.isPending} onClick={onClose}>
            <Trans>Cancel</Trans>
          </Button>
          <Button
            type="button"
            disabled={create.isPending || selectedIds.length === 0}
            onClick={() => create.mutate()}
          >
            {create.isPending ? <Trans>Adding…</Trans> : <Trans>Add to Home</Trans>}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
