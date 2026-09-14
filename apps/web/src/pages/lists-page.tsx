import { Plural, Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery } from "@tanstack/react-query";
import { CheckIcon, ListIcon, PlusIcon, SparklesIcon, XIcon } from "lucide-react";
import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { toast } from "sonner";

import { errorMessage } from "@/api/client";
import { useInvalidateLists } from "@/api/invalidate-lists";
import { addList } from "@/api/lists";
import { configQuery, listsQuery } from "@/api/queries";
import { addSmartList } from "@/api/smart-lists";
import { AppFrame } from "@/components/layout/app-frame";
import { PageContainer } from "@/components/layout/page-container";
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
import { Placeholder } from "@/components/ui/placeholder";
import { Select } from "@/components/ui/select";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import { useCapabilities } from "@/lib/capabilities";

export function ListsPage() {
  const lists = useQuery(listsQuery());
  const capabilities = useCapabilities();
  const contentWritable = capabilities.contentWritable;
  const [createOpen, setCreateOpen] = useState(false);
  const items = lists.data?.items ?? [];

  return (
    <AppFrame error={lists.error ? errorMessage(lists.error) : undefined}>
      <PageContainer>
        <header className="flex items-center justify-end gap-2">
          <Button
            type="button"
            size="sm"
            disabled={!contentWritable}
            onClick={() => setCreateOpen(true)}
          >
            <PlusIcon data-icon="inline-start" />
            <Trans>New list</Trans>
          </Button>
        </header>

        {lists.isPending ? (
          <Placeholder>
            <Trans>Loading…</Trans>
          </Placeholder>
        ) : items.length === 0 ? (
          <Placeholder>
            <Trans>No lists yet. Create one to start collecting entities.</Trans>
          </Placeholder>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
            {items.map((list) => (
              <Link
                // Static and smart lists are separate id namespaces, so the
                // key needs the kind too.
                key={`${list.kind}:${list.id}`}
                to={
                  list.kind === "smart"
                    ? `/lists/smart/${encodeURIComponent(list.id)}`
                    : `/lists/${encodeURIComponent(list.id)}`
                }
                className="flex min-h-32 flex-col gap-2 rounded-md border p-4 transition-colors hover:bg-accent"
              >
                <div className="flex items-center gap-2">
                  {list.kind === "smart" ? (
                    <SparklesIcon className="size-4 shrink-0 text-muted-foreground" />
                  ) : (
                    <ListIcon className="size-4 shrink-0 text-muted-foreground" />
                  )}
                  <span className="truncate font-medium">{list.name}</span>
                </div>
                {list.description ? (
                  <p className="line-clamp-3 text-xs text-muted-foreground">{list.description}</p>
                ) : null}
                <div className="mt-auto flex items-center gap-2 text-xs text-muted-foreground">
                  <Badge variant="outline">
                    <Plural value={list.itemCount} one="# item" other="# items" />
                  </Badge>
                  {list.sectionCount > 0 ? (
                    <Badge variant="outline">
                      <Plural value={list.sectionCount} one="# section" other="# sections" />
                    </Badge>
                  ) : null}
                  {list.kind === "smart" ? (
                    <Badge variant="outline">
                      <Trans comment="Badge on a list card marking a smart list (criteria-driven, updates automatically)">
                        Smart
                      </Trans>
                    </Badge>
                  ) : null}
                </div>
              </Link>
            ))}
          </div>
        )}
      </PageContainer>

      <CreateListDialog open={createOpen} onOpenChange={setCreateOpen} />
    </AppFrame>
  );
}

function CreateListDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useLingui();
  const navigate = useNavigate();
  const invalidateLists = useInvalidateLists();
  const config = useQuery(configQuery());
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"static" | "smart">("static");
  const [scope, setScope] = useState("");
  const validationError = name.trim()
    ? basenameValidationError(normalizeBasename(name))
    : undefined;

  const reset = () => {
    setName("");
    setKind("static");
    setScope("");
  };

  const create = useMutation({
    mutationFn: async () => {
      const normalized = normalizeBasename(name);
      if (kind === "smart") {
        const list = await addSmartList({ name: normalized, scope: scope || undefined });
        return { id: list.id, href: `/lists/smart/${encodeURIComponent(list.id)}` };
      }
      const list = await addList({ name: normalized });
      return { id: list.id, href: `/lists/${encodeURIComponent(list.id)}` };
    },
    onSuccess: async (created) => {
      toast.success(kind === "smart" ? t`Smart list created` : t`List created`);
      onOpenChange(false);
      reset();
      await invalidateLists();
      navigate(created.href);
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        onOpenChange(next);
        if (!next) reset();
      }}
    >
      <DialogContent aria-describedby={undefined}>
        <DialogHeader>
          <DialogTitle>
            <Trans>New list</Trans>
          </DialogTitle>
        </DialogHeader>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!name.trim() || validationError) return;
            create.mutate();
          }}
          className="flex flex-col gap-2"
        >
          <div className="grid grid-cols-2 gap-2">
            <Button
              type="button"
              variant={kind === "static" ? "secondary" : "outline"}
              onClick={() => setKind("static")}
            >
              <ListIcon data-icon="inline-start" />
              <Trans>List</Trans>
            </Button>
            <Button
              type="button"
              variant={kind === "smart" ? "secondary" : "outline"}
              onClick={() => setKind("smart")}
            >
              <SparklesIcon data-icon="inline-start" />
              <Trans>Smart list</Trans>
            </Button>
          </div>
          <label className="text-sm font-medium">
            <Trans>Name</Trans>
            <Input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder={t`Watchlist`}
              autoFocus
              aria-invalid={Boolean(validationError)}
            />
          </label>
          {kind === "smart" ? (
            <label className="text-sm font-medium">
              <Trans>Scope</Trans>
              <Select
                value={scope}
                className="mt-1 w-full"
                onChange={(event) => setScope(event.target.value)}
              >
                <option value="">{t`All types`}</option>
                {(config.data?.types ?? []).map((type) => (
                  <option key={type.id} value={type.id}>
                    {type.label}
                  </option>
                ))}
              </Select>
            </label>
          ) : null}
          {validationError ? <p className="text-xs text-destructive">{validationError}</p> : null}
          <DialogFooter className="mt-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={create.isPending}
            >
              <XIcon data-icon="inline-start" />
              <Trans>Cancel</Trans>
            </Button>
            <Button
              type="submit"
              disabled={!name.trim() || Boolean(validationError) || create.isPending}
            >
              <CheckIcon data-icon="inline-start" />
              {create.isPending ? <Trans>Creating…</Trans> : <Trans>Create</Trans>}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
