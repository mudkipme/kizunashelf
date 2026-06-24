import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CheckIcon, ListIcon, PlusIcon, XIcon } from "lucide-react";
import { Link, useNavigate } from "react-router-dom";

import { errorMessage } from "@/api/client";
import { addList } from "@/api/lists";
import { capabilitiesQuery, listsQuery, queryKeys } from "@/api/queries";
import { AppFrame } from "@/components/layout/app-frame";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";

export function ListsPage() {
  const lists = useQuery(listsQuery());
  const capabilities = useQuery(capabilitiesQuery());
  const contentWritable = capabilities.data?.contentWritable !== false;
  const [createOpen, setCreateOpen] = useState(false);
  const [error, setError] = useState<string>();
  const items = lists.data?.items ?? [];

  return (
    <AppFrame error={error ?? (lists.error ? errorMessage(lists.error) : undefined)}>
      <div className="mx-auto flex w-full max-w-5xl flex-col gap-4 p-4">
        <header className="flex items-center justify-between gap-2">
          <div>
            <h1 className="text-lg font-semibold">Lists</h1>
            <p className="text-xs text-muted-foreground">
              Curated collections of your items, saved as plain files you own.
            </p>
          </div>
          <Button type="button" size="sm" disabled={!contentWritable} onClick={() => setCreateOpen(true)}>
            <PlusIcon data-icon="inline-start" />
            New list
          </Button>
        </header>

        {lists.isPending ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">Loading</div>
        ) : items.length === 0 ? (
          <div className="rounded-md border p-8 text-center text-sm text-muted-foreground">
            No lists yet. Create one to start collecting entities.
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
            {items.map((list) => (
              <Link
                key={list.id}
                to={`/lists/${encodeURIComponent(list.id)}`}
                className="flex min-h-32 flex-col gap-2 rounded-md border p-4 transition-colors hover:bg-accent"
              >
                <div className="flex items-center gap-2">
                  <ListIcon className="size-4 shrink-0 text-muted-foreground" />
                  <span className="truncate font-medium">{list.name}</span>
                </div>
                {list.description ? (
                  <p className="line-clamp-3 text-xs text-muted-foreground">{list.description}</p>
                ) : null}
                <div className="mt-auto flex items-center gap-2 text-xs text-muted-foreground">
                  <Badge variant="outline">
                    {list.itemCount} {list.itemCount === 1 ? "item" : "items"}
                  </Badge>
                  <Badge variant="outline">{list.ordered ? "Ordered" : "Unordered"}</Badge>
                </div>
              </Link>
            ))}
          </div>
        )}
      </div>

      <CreateListDialog open={createOpen} onOpenChange={setCreateOpen} onError={setError} />
    </AppFrame>
  );
}

function CreateListDialog({
  open,
  onOpenChange,
  onError,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onError: (message?: string) => void;
}) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const validationError = name.trim() ? basenameValidationError(normalizeBasename(name)) : undefined;

  const create = useMutation({
    mutationFn: () => addList({ name: normalizeBasename(name) }),
    onSuccess: async (list) => {
      onError(undefined);
      onOpenChange(false);
      setName("");
      await queryClient.invalidateQueries({ queryKey: queryKeys.lists });
      navigate(`/lists/${encodeURIComponent(list.id)}`);
    },
    onError: (mutationError) => onError(errorMessage(mutationError)),
  });

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        onOpenChange(next);
        if (!next) setName("");
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New list</DialogTitle>
          <DialogDescription>Give your list a name.</DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!name.trim() || validationError) return;
            create.mutate();
          }}
          className="flex flex-col gap-2"
        >
          <label className="text-sm font-medium">
            Name
            <Input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Watchlist"
              autoFocus
              aria-invalid={Boolean(validationError)}
            />
          </label>
          {validationError ? <p className="text-xs text-destructive">{validationError}</p> : null}
          <DialogFooter className="mt-2">
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)} disabled={create.isPending}>
              <XIcon data-icon="inline-start" />
              Cancel
            </Button>
            <Button type="submit" disabled={!name.trim() || Boolean(validationError) || create.isPending}>
              <CheckIcon data-icon="inline-start" />
              {create.isPending ? "Creating" : "Create"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
