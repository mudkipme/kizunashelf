import { useEffect, useMemo, useState } from "react";
import { Plural, Trans, useLingui } from "@lingui/react/macro";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  CheckIcon,
  FilePenLineIcon,
  LayoutGridIcon,
  ListIcon,
  PencilIcon,
  SparklesIcon,
  Trash2Icon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react";
import { useNavigate, useParams, useSearchParams } from "react-router-dom";
import { toast } from "sonner";

import { errorMessage, isConflictError } from "@/api/client";
import {
  allTagsQuery,
  configQuery,
  queryKeys,
  smartListPreviewQuery,
  smartListQuery,
  smartListResultsQuery,
} from "@/api/queries";
import { removeSmartList, saveSmartList } from "@/api/smart-lists";
import { EntityResults } from "@/components/assets/entity-results";
import { AppFrame } from "@/components/layout/app-frame";
import { PageContainer } from "@/components/layout/page-container";
import { CriteriaSummary } from "@/components/smart-lists/criteria-summary";
import { RuleBuilder } from "@/components/smart-lists/rule-builder";
import { ruleFieldMetas } from "@/components/smart-lists/rule-field-meta";
import { pruneIncompleteRules } from "@/components/smart-lists/rule-model";
import { SortPicker } from "@/components/smart-lists/sort-picker";
import { Alert } from "@/components/ui/alert";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
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
import { Placeholder } from "@/components/ui/placeholder";
import { Select } from "@/components/ui/select";
import { useDebouncedValue } from "@/hooks/use-debounce";
import { basenameValidationError, normalizeBasename } from "@/lib/basename";
import { useCapabilities } from "@/lib/capabilities";
import { pageSize } from "@/lib/constants";
import { useTitleLanguage } from "@/lib/language";
import { fieldLabelsByType, typeHasCoverField } from "@/lib/type-config";
import type {
  SmartFilterGroup,
  SmartListDetail,
  SmartListView,
  TypeConfig,
} from "@/types/api";

type Draft = {
  scope?: string;
  filters: SmartFilterGroup;
  views: SmartListView[];
};

export function SmartListPage() {
  const { id = "" } = useParams();
  const { t } = useLingui();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [searchParams, setSearchParams] = useSearchParams();
  const language = useTitleLanguage();
  const config = useQuery(configQuery());
  const allTagsData = useQuery(allTagsQuery()).data?.tags;
  const allTags = useMemo(() => allTagsData ?? [], [allTagsData]);
  const capabilities = useCapabilities();
  const contentWritable = capabilities.contentWritable;

  const detail = useQuery(smartListQuery(id));
  const data = detail.data;

  // Editing works on a draft copy; `null` = read mode. The draft feeds the
  // preview endpoint so results track the rules live, before anything is saved.
  const [draft, setDraft] = useState<Draft | null>(null);
  const [draftRevision, setDraftRevision] = useState<string | null>(null);
  const editing = draft !== null;
  const externalChange = Boolean(editing && data && draftRevision !== data.revision);
  const [renameOpen, setRenameOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const views = draft?.views ?? data?.views ?? [];
  const viewParam = searchParams.get("view") ?? undefined;
  const activeViewIndex = Math.max(
    0,
    views.findIndex((view) => view.name === viewParam),
  );
  const activeView = views[activeViewIndex] as SmartListView | undefined;
  const page = Math.max(1, Number(searchParams.get("page")) || 1);

  const savedResults = useQuery({
    ...smartListResultsQuery(id, {
      view: activeView && !editing ? activeView.name : undefined,
      page,
      pageSize,
      titleLanguage: language,
    }),
    enabled: !editing,
  });
  const debouncedDraft = useDebouncedValue(draft, 350);
  const previewResults = useQuery({
    ...smartListPreviewQuery({
      scope: debouncedDraft?.scope,
      filters: pruneIncompleteRules(debouncedDraft?.filters ?? { conjunction: "all", rules: [] }),
      sort: debouncedDraft?.views[activeViewIndex]?.sort ?? [],
      limit: debouncedDraft?.views[activeViewIndex]?.limit ?? undefined,
      page,
      pageSize,
      titleLanguage: language,
    }),
    enabled: editing && debouncedDraft !== null,
  });
  const results = editing ? previewResults : savedResults;

  const configTypes = config.data?.types;
  const typeConfigs = useMemo(() => configTypes ?? [], [configTypes]);
  const scope = editing ? draft.scope : data?.scope;
  const scopeConfig = scope ? typeConfigs.find((type) => type.id === scope) : undefined;
  const tagsField = config.data?.tagsField ?? undefined;
  // Only a scoped list offers frontmatter fields — for the criteria and for
  // the sort key alike. See `ruleFieldMetas`.
  const fieldMetas = useMemo(
    () => ruleFieldMetas(scopeConfig, tagsField, allTags, t),
    [scopeConfig, tagsField, allTags, t],
  );

  const fieldLabels = useMemo(() => fieldLabelsByType(config.data?.types), [config.data]);
  const showCover = !scope || typeHasCoverField(scopeConfig);
  const showType = !scope;

  const entities = results.data?.items ?? [];
  const total = results.data?.total ?? 0;
  const totalPages = results.data?.totalPages ?? 1;

  const setParam = (key: string, value: string | undefined) => {
    setSearchParams(
      (params) => {
        const next = new URLSearchParams(params);
        if (value === undefined) next.delete(key);
        else next.set(key, value);
        return next;
      },
      { replace: true },
    );
  };

  const invalidate = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.smartList(id) }),
      queryClient.invalidateQueries({ queryKey: ["smartListResults", id] }),
      queryClient.invalidateQueries({ queryKey: queryKeys.lists }),
    ]);
  };

  const save = useMutation({
    mutationFn: (draftToSave: Draft) =>
      saveSmartList(id, {
        revision: draftRevision ?? data?.revision ?? "",
        scope: draftToSave.scope,
        filters: pruneIncompleteRules(draftToSave.filters),
        views: draftToSave.views,
      }),
    onSuccess: async () => {
      setDraft(null);
      setDraftRevision(null);
      await invalidate();
      toast.success(t`Smart list saved`);
    },
    onError: (error) => {
      if (isConflictError(error)) void detail.refetch();
      toast.error(
        isConflictError(error)
          ? t`This smart list changed on disk. Reload the page and redo your edits.`
          : errorMessage(error),
      );
    },
  });

  const destroy = useMutation({
    mutationFn: () => removeSmartList(id),
    onSuccess: async () => {
      toast.success(t`Smart list deleted`);
      await queryClient.invalidateQueries({ queryKey: queryKeys.lists });
      navigate("/lists");
    },
    onError: (error) => toast.error(errorMessage(error)),
  });

  const startEditing = () => {
    if (!data) return;
    setDraftRevision(data.revision);
    setDraft({
      scope: data.scope ?? undefined,
      filters: structuredClone(data.filters),
      views: structuredClone(data.views),
    });
  };

  const reloadLatest = () => {
    if (!data) return;
    setDraftRevision(data.revision);
    setDraft({
      scope: data.scope ?? undefined,
      filters: structuredClone(data.filters),
      views: structuredClone(data.views),
    });
  };

  return (
    <AppFrame
      error={
        detail.error
          ? errorMessage(detail.error)
          : results.error
            ? errorMessage(results.error)
            : undefined
      }
    >
      <PageContainer>
        {detail.isPending ? (
          <Placeholder>
            <Trans>Loading…</Trans>
          </Placeholder>
        ) : !data ? (
          <Placeholder>
            <Trans>Smart list not found</Trans>
          </Placeholder>
        ) : (
          <>
            <header className="flex flex-wrap items-center gap-2">
              <SparklesIcon className="size-5 shrink-0 text-muted-foreground" />
              <h1 className="truncate text-lg font-semibold">{data.name}</h1>
              {scopeConfig ? <Badge variant="outline">{scopeConfig.label}</Badge> : null}
              <span className="mr-auto text-xs text-muted-foreground">
                <Plural value={total} one="# match" other="# matches" />
              </span>
              {editing ? (
                <>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={save.isPending}
                    onClick={() => {
                      setDraft(null);
                      setDraftRevision(null);
                    }}
                  >
                    <XIcon data-icon="inline-start" />
                    <Trans>Cancel</Trans>
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    disabled={save.isPending || externalChange}
                    onClick={() => draft && save.mutate(draft)}
                  >
                    <CheckIcon data-icon="inline-start" />
                    {save.isPending ? <Trans>Saving…</Trans> : <Trans>Save</Trans>}
                  </Button>
                </>
              ) : (
                <>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={!contentWritable}
                    onClick={startEditing}
                  >
                    <PencilIcon data-icon="inline-start" />
                    <Trans>Edit criteria</Trans>
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={!contentWritable}
                    onClick={() => setRenameOpen(true)}
                  >
                    <FilePenLineIcon data-icon="inline-start" />
                    <Trans>Rename</Trans>
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={!contentWritable || destroy.isPending}
                    onClick={() => setDeleteOpen(true)}
                  >
                    <Trash2Icon data-icon="inline-start" />
                    <Trans>Delete</Trans>
                  </Button>
                </>
              )}
              {views.length > 0 ? (
                <div className="flex items-center gap-1 rounded-md border p-0.5">
                  {views.map((view) => (
                    <Button
                      key={view.name}
                      type="button"
                      size="sm"
                      variant={view.name === activeView?.name ? "secondary" : "ghost"}
                      onClick={() => {
                        setParam("page", undefined);
                        setParam("view", view.name);
                      }}
                    >
                      {view.layout === "grid" ? (
                        <LayoutGridIcon data-icon="inline-start" />
                      ) : (
                        <ListIcon data-icon="inline-start" />
                      )}
                      {view.name}
                    </Button>
                  ))}
                </div>
              ) : null}
            </header>

            {externalChange ? (
              <Alert className="flex flex-wrap items-center justify-between gap-3">
                <span className="min-w-0">
                  <Trans comment="Warning banner in the smart-list editor after another app changes its .base file; the local draft has been preserved">
                    This smart list changed on disk. Your unsaved edits are still here.
                  </Trans>
                </span>
                <Button type="button" variant="outline" size="sm" onClick={reloadLatest}>
                  <Trans>Reload latest version</Trans>
                </Button>
              </Alert>
            ) : null}

            {editing && draft ? (
              <EditPanel
                draft={draft}
                typeConfigs={typeConfigs}
                fieldMetas={fieldMetas}
                scopeConfig={scopeConfig}
                activeViewIndex={activeViewIndex}
                disabled={save.isPending || externalChange}
                onChange={setDraft}
              />
            ) : (
              <CriteriaSummary
                group={data.filters}
                empty={
                  data.scope ? (
                    <Trans>No further criteria — every entity of this type matches.</Trans>
                  ) : (
                    <Trans>No criteria — everything in the library matches.</Trans>
                  )
                }
              />
            )}

            {(data.warnings?.length ?? 0) > 0 ? (
              <WarningsBanner warnings={data.warnings ?? []} />
            ) : null}

            <EntityResults
              className="rounded-md border"
              entities={entities}
              layout={activeView?.layout ?? "list"}
              labelsByType={fieldLabels}
              showCover={showCover}
              showType={showType}
              loading={results.isFetching}
              page={results.data?.page ?? page}
              totalPages={totalPages}
              total={total}
              empty={<Trans>No entries match the criteria</Trans>}
              onPageChange={(next) => setParam("page", next > 1 ? String(next) : undefined)}
            />
          </>
        )}
      </PageContainer>

      <RenameSmartListDialog
        open={renameOpen}
        onOpenChange={setRenameOpen}
        detail={data}
        listId={id}
      />
      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              <Trans>Delete this smart list?</Trans>
            </AlertDialogTitle>
            <AlertDialogDescription>
              <Trans>
                The .base file moves to the vault's trash folder. Entities it matches are not
                touched.
              </Trans>
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>
              <Trans>Cancel</Trans>
            </AlertDialogCancel>
            <AlertDialogAction onClick={() => destroy.mutate()}>
              <Trans>Delete</Trans>
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </AppFrame>
  );
}

/// The edit surface: type scope, the rule builder, and the active view's sort
/// and limit. Everything edits the draft; the preview endpoint renders it live.
function EditPanel({
  draft,
  typeConfigs,
  fieldMetas,
  scopeConfig,
  activeViewIndex,
  disabled,
  onChange,
}: {
  draft: Draft;
  typeConfigs: TypeConfig[];
  fieldMetas: ReturnType<typeof ruleFieldMetas>;
  scopeConfig: TypeConfig | undefined;
  activeViewIndex: number;
  disabled: boolean;
  onChange: (draft: Draft) => void;
}) {
  const { t } = useLingui();
  const activeView = draft.views[activeViewIndex] as SmartListView | undefined;
  const sortSpec = activeView?.sort?.[0];

  const updateActiveView = (patch: Partial<SmartListView>) => {
    if (!activeView) return;
    onChange({
      ...draft,
      views: draft.views.map((view, index) =>
        index === activeViewIndex ? { ...view, ...patch } : view,
      ),
    });
  };

  return (
    <div className="flex flex-col gap-3 rounded-md border p-3">
      <div className="flex flex-wrap items-center gap-2">
        <label className="text-xs font-medium text-muted-foreground">
          <Trans>Scope</Trans>
        </label>
        <Select
          value={draft.scope ?? ""}
          disabled={disabled}
          aria-label={t`Scope`}
          className="w-fit min-w-0"
          onChange={(event) =>
            onChange({ ...draft, scope: event.target.value || undefined })
          }
        >
          <option value="">{t`All types`}</option>
          {typeConfigs.map((type) => (
            <option key={type.id} value={type.id}>
              {type.label}
            </option>
          ))}
        </Select>
        {activeView ? (
          <>
            <label className="ml-auto text-xs font-medium text-muted-foreground">
              <Trans comment="Label before the sort-key picker of the active smart-list view">
                Sort {activeView.name} by
              </Trans>
            </label>
            <SortPicker
              typeConfig={scopeConfig}
              sort={sortSpec}
              disabled={disabled}
              onChange={(sort) => updateActiveView({ sort: sort ? [sort] : [] })}
            />
            <Input
              type="number"
              min={1}
              value={activeView.limit ?? ""}
              placeholder={t`No limit`}
              disabled={disabled}
              aria-label={t`Limit`}
              className="w-24"
              onChange={(event) =>
                updateActiveView({
                  limit: event.target.value ? Math.max(1, Number(event.target.value)) : undefined,
                })
              }
            />
          </>
        ) : null}
      </div>
      {draft.scope ? null : (
        <p className="text-xs text-muted-foreground">
          <Trans>
            Pick a scope to filter on that type's own fields — across all types only the
            properties every entry has can be matched.
          </Trans>
        </p>
      )}
      <RuleBuilder
        fieldMetas={fieldMetas}
        value={draft.filters}
        disabled={disabled}
        onChange={(filters) => onChange({ ...draft, filters })}
      />
    </div>
  );
}

function RenameSmartListDialog({
  open,
  onOpenChange,
  detail,
  listId,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  detail: SmartListDetail | undefined;
  listId: string;
}) {
  const { t } = useLingui();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  useEffect(() => {
    if (open) setName(detail?.name ?? "");
  }, [open, detail?.name]);
  const validationError = name.trim() ? basenameValidationError(normalizeBasename(name)) : undefined;

  const rename = useMutation({
    // A rename is a save that carries `renameTo` and echoes the current
    // criteria back unchanged (unsupported rules round-trip via `raw`).
    mutationFn: () =>
      saveSmartList(listId, {
        revision: detail?.revision ?? "",
        scope: detail?.scope ?? undefined,
        filters: detail?.filters ?? { conjunction: "all", rules: [] },
        views: detail?.views ?? [],
        renameTo: normalizeBasename(name),
      }),
    onSuccess: async (updated) => {
      toast.success(t`Smart list renamed`);
      onOpenChange(false);
      await queryClient.invalidateQueries({ queryKey: queryKeys.lists });
      navigate(`/lists/smart/${encodeURIComponent(updated.id)}`, { replace: true });
    },
    onError: (error) => {
      toast.error(
        isConflictError(error)
          ? t`A list with this name already exists, or the file changed on disk.`
          : errorMessage(error),
      );
    },
  });

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            <Trans>Rename smart list</Trans>
          </DialogTitle>
          <DialogDescription>
            <Trans>Renames the .base file in your vault.</Trans>
          </DialogDescription>
        </DialogHeader>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!name.trim() || validationError) return;
            rename.mutate();
          }}
          className="flex flex-col gap-2"
        >
          <label className="text-sm font-medium">
            <Trans>Name</Trans>
            <Input
              value={name}
              onChange={(event) => setName(event.target.value)}
              autoFocus
              aria-invalid={Boolean(validationError)}
            />
          </label>
          {validationError ? <p className="text-xs text-destructive">{validationError}</p> : null}
          <DialogFooter className="mt-2">
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={rename.isPending}
            >
              <XIcon data-icon="inline-start" />
              <Trans>Cancel</Trans>
            </Button>
            <Button
              type="submit"
              disabled={!name.trim() || Boolean(validationError) || rename.isPending}
            >
              <CheckIcon data-icon="inline-start" />
              {rename.isPending ? <Trans>Renaming…</Trans> : <Trans>Rename</Trans>}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/// The parts of the underlying `.base` file the app ignores (unsupported
/// filters, views, sorts). They still apply in Obsidian and are preserved on
/// every save, so surface them rather than silently diverging.
function WarningsBanner({ warnings }: { warnings: string[] }) {
  return (
    <div className="flex gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-xs">
      <TriangleAlertIcon className="mt-0.5 size-4 shrink-0 text-amber-600 dark:text-amber-400" />
      <div className="flex flex-col gap-1">
        <p className="font-medium">
          <Trans>Some of this file's syntax isn't supported here and is ignored:</Trans>
        </p>
        <ul className="list-inside list-disc text-muted-foreground">
          {warnings.map((warning) => (
            <li key={warning} className="break-all">
              {warning}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
