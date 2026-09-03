import { Trans, useLingui } from "@lingui/react/macro";
import { SearchIcon, WandSparklesIcon } from "lucide-react";
import { memo } from "react";

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
import { Select } from "@/components/ui/select";
import {
  externalSourceLabel,
  type ExternalBodyPreviewEntry,
  type ExternalMetadataPreviewEntry,
} from "@/lib/external-metadata";
import { cn } from "@/lib/utils";
import type { ExternalMatch, ExternalProviderCatalog } from "@/types/api";

type ExternalRefAction = {
  field: string;
  provider: string;
  value: string;
};

type ExternalMatchDialogProps = {
  open: boolean;
  query: string;
  provider: string;
  candidates: ExternalMatch[];
  selectedCandidate?: ExternalMatch;
  metadataEntries: ExternalMetadataPreviewEntry[];
  bodyEntries: ExternalBodyPreviewEntry[];
  selectedFields: Set<string>;
  selectedBodySections: Set<string>;
  providerCatalog?: ExternalProviderCatalog;
  providerOptions: string[];
  externalSearchEnabled: boolean;
  currentValues?: Record<string, unknown>;
  // Locked checkboxes and replace-vs-append badges, from the core's review of
  // the chosen candidate (`reviewExternalCandidate`). `sectionModes` is
  // undefined until the review resolves; the badges are hidden meanwhile.
  fieldLocks?: ReadonlySet<string>;
  sectionLocks?: ReadonlySet<string>;
  sectionModes?: Record<string, "replace" | "append">;
  existingExternalRefs?: ExternalRefAction[];
  searching: boolean;
  applying: boolean;
  contentWritable: boolean;
  emptyMessage?: string;
  coverDownloadAvailable?: boolean;
  downloadCover?: boolean;
  onDownloadCoverChange?: (value: boolean) => void;
  onOpenChange: (open: boolean) => void;
  onQueryChange: (value: string) => void;
  onProviderChange: (value: string) => void;
  onSearch: () => void;
  onRefreshRef?: (provider: string, value: string) => void;
  onChooseCandidate: (match: ExternalMatch) => void;
  onSelectedFieldsChange: (fields: Set<string>) => void;
  onSelectedBodySectionsChange: (sections: Set<string>) => void;
  onApply: () => void;
};

export function ExternalMatchDialog({
  open,
  query,
  provider,
  candidates,
  selectedCandidate,
  metadataEntries,
  bodyEntries,
  selectedFields,
  selectedBodySections,
  providerCatalog,
  providerOptions,
  externalSearchEnabled,
  currentValues,
  fieldLocks,
  sectionLocks,
  sectionModes,
  existingExternalRefs = [],
  searching,
  applying,
  contentWritable,
  emptyMessage,
  coverDownloadAvailable = false,
  downloadCover = false,
  onDownloadCoverChange,
  onOpenChange,
  onQueryChange,
  onProviderChange,
  onSearch,
  onRefreshRef,
  onChooseCandidate,
  onSelectedFieldsChange,
  onSelectedBodySectionsChange,
  onApply,
}: ExternalMatchDialogProps) {
  const { t } = useLingui();
  const selectedCount = selectedFields.size + selectedBodySections.size;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        aria-describedby={undefined}
        className="top-0 left-0 flex h-dvh max-w-none translate-x-0 translate-y-0 flex-col gap-0 rounded-none border-0 p-0 sm:top-[50%] sm:left-[50%] sm:h-[min(760px,calc(100dvh-2rem))] sm:w-[min(1100px,calc(100vw-2rem))] sm:max-w-none sm:translate-x-[-50%] sm:translate-y-[-50%] sm:rounded-md sm:border"
      >
        <DialogHeader className="border-b px-4 py-4 pr-12 sm:px-6">
          <DialogTitle>
            <Trans>External Match</Trans>
          </DialogTitle>
        </DialogHeader>

        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4 sm:p-6">
          <div className="flex flex-col gap-2 md:flex-row md:items-end">
            <label className="flex min-w-0 flex-1 flex-col gap-1 text-sm font-medium">
              <Trans>Search</Trans>
              <Input value={query} onChange={(event) => onQueryChange(event.target.value)} />
            </label>
            <Select
              className="md:w-44"
              value={provider}
              onChange={(event) => onProviderChange(event.target.value)}
              aria-label={t`Provider`}
              disabled={!externalSearchEnabled}
            >
              {providerOptions.length === 0 ? (
                <option value="all">{t`No supported providers`}</option>
              ) : null}
              {providerOptions.length > 1 ? <option value="all">{t`All providers`}</option> : null}
              {providerOptions.map((providerOption) => (
                <option key={providerOption} value={providerOption}>
                  {externalSourceLabel(providerCatalog, providerOption)}
                </option>
              ))}
            </Select>
            <Button
              type="button"
              variant="outline"
              onClick={onSearch}
              disabled={searching || !externalSearchEnabled}
            >
              <SearchIcon data-icon="inline-start" />
              {searching ? t`Searching…` : t`Search`}
            </Button>
          </div>

          {existingExternalRefs.length > 0 ? (
            <div className="flex flex-wrap gap-2">
              {existingExternalRefs.map((ref) => (
                <Button
                  key={`${ref.provider}:${ref.field}`}
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => onRefreshRef?.(ref.provider, ref.value)}
                  disabled={searching || !onRefreshRef}
                >
                  <WandSparklesIcon data-icon="inline-start" />
                  <Trans>Refresh {externalSourceLabel(providerCatalog, ref.provider)}</Trans>
                </Button>
              ))}
            </div>
          ) : null}

          <div className="grid min-h-0 gap-3 min-[900px]:grid-cols-[minmax(260px,1fr)_minmax(300px,380px)]">
            <CandidateList
              candidates={candidates}
              selectedCandidate={selectedCandidate}
              providerCatalog={providerCatalog}
              emptyMessage={emptyMessage ?? t`No candidates loaded`}
              onChooseCandidate={onChooseCandidate}
            />

            <SelectedMetadataPanel
              selectedCandidate={selectedCandidate}
              metadataEntries={metadataEntries}
              bodyEntries={bodyEntries}
              selectedFields={selectedFields}
              selectedBodySections={selectedBodySections}
              currentValues={currentValues}
              fieldLocks={fieldLocks}
              sectionLocks={sectionLocks}
              sectionModes={sectionModes}
              contentWritable={contentWritable}
              onSelectedFieldsChange={onSelectedFieldsChange}
              onSelectedBodySectionsChange={onSelectedBodySectionsChange}
            />
          </div>
        </div>

        <DialogFooter className="border-t p-4 sm:flex-row sm:items-center sm:justify-between sm:px-6">
          {coverDownloadAvailable ? (
            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <input
                type="checkbox"
                checked={downloadCover}
                onChange={(event) => onDownloadCoverChange?.(event.target.checked)}
                disabled={!contentWritable}
              />
              <Trans>Download cover locally</Trans>
            </label>
          ) : (
            <span className="hidden sm:block" />
          )}
          <Button
            type="button"
            onClick={onApply}
            disabled={!contentWritable || applying || !selectedCandidate || selectedCount === 0}
          >
            <WandSparklesIcon data-icon="inline-start" />
            {applying ? t`Applying…` : t`Apply Selected`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// The candidate list and metadata panel are memoized so typing in the search
// box (which only changes `query`) does not re-render either subtree.
const CandidateList = memo(function CandidateList({
  candidates,
  selectedCandidate,
  providerCatalog,
  emptyMessage,
  onChooseCandidate,
}: {
  candidates: ExternalMatch[];
  selectedCandidate?: ExternalMatch;
  providerCatalog?: ExternalProviderCatalog;
  emptyMessage: string;
  onChooseCandidate: (match: ExternalMatch) => void;
}) {
  return (
    <div className="grid content-start gap-2">
      {candidates.map((match) => {
        const candidate = match.candidate;
        const selected =
          selectedCandidate?.candidate.provider === candidate.provider &&
          selectedCandidate.candidate.sourceId === candidate.sourceId;
        return (
          <button
            key={`${candidate.provider}:${candidate.sourceId}`}
            type="button"
            className={cn(
              "min-w-0 rounded-md border p-3 text-left transition-colors hover:bg-accent",
              selected && "border-primary bg-accent",
            )}
            onClick={() => onChooseCandidate(match)}
          >
            <div className="flex min-w-0 items-center gap-2">
              <Badge variant="secondary">
                {externalSourceLabel(providerCatalog, candidate.provider)}
              </Badge>
              <span className="min-w-0 truncate text-sm font-medium">{candidate.title}</span>
            </div>
            {candidate.brief ? (
              <p className="mt-2 line-clamp-2 text-xs text-muted-foreground">{candidate.brief}</p>
            ) : null}
          </button>
        );
      })}
      {candidates.length === 0 ? (
        <div className="rounded-md border p-4 text-sm text-muted-foreground">{emptyMessage}</div>
      ) : null}
    </div>
  );
});

const SelectedMetadataPanel = memo(function SelectedMetadataPanel({
  selectedCandidate,
  metadataEntries,
  bodyEntries,
  selectedFields,
  selectedBodySections,
  currentValues,
  fieldLocks,
  sectionLocks,
  sectionModes,
  contentWritable,
  onSelectedFieldsChange,
  onSelectedBodySectionsChange,
}: {
  selectedCandidate?: ExternalMatch;
  metadataEntries: ExternalMetadataPreviewEntry[];
  bodyEntries: ExternalBodyPreviewEntry[];
  selectedFields: Set<string>;
  selectedBodySections: Set<string>;
  currentValues?: Record<string, unknown>;
  fieldLocks?: ReadonlySet<string>;
  sectionLocks?: ReadonlySet<string>;
  sectionModes?: Record<string, "replace" | "append">;
  contentWritable: boolean;
  onSelectedFieldsChange: (fields: Set<string>) => void;
  onSelectedBodySectionsChange: (sections: Set<string>) => void;
}) {
  const { t } = useLingui();
  function toggleField(field: string) {
    const next = new Set(selectedFields);
    if (next.has(field)) next.delete(field);
    else next.add(field);
    onSelectedFieldsChange(next);
  }

  function toggleBodySection(section: string) {
    const next = new Set(selectedBodySections);
    if (next.has(section)) next.delete(section);
    else next.add(section);
    onSelectedBodySectionsChange(next);
  }

  return (
    <div className="rounded-md border p-3">
      <h3 className="text-sm font-semibold">
        <Trans>Selected Metadata</Trans>
      </h3>
      {selectedCandidate ? (
        <div className="mt-3 flex flex-col gap-2">
          {metadataEntries.map((entry) => {
            const newValue = entry.hasValue
              ? formatMetadataValue(entry.value)
              : t`No value returned`;
            // The external ref is forced on and no-op values forced off; empty
            // fields default on, existing ones off — the core's review supplies
            // both the default selection and these locks.
            const locked = fieldLocks?.has(entry.field) ?? false;
            return (
              <label key={entry.field} className="flex min-w-0 items-start gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={selectedFields.has(entry.field)}
                  onChange={() => toggleField(entry.field)}
                  className="mt-1"
                  disabled={!contentWritable || locked}
                />
                <span className="min-w-0">
                  <span className="block font-medium">{entry.label}</span>
                  <span className="block text-xs text-muted-foreground">
                    {entry.externalField ?? t`external ref`}
                  </span>
                  {currentValues ? (
                    <span className="block text-xs break-words text-muted-foreground">
                      <Trans>Current: {formatMetadataValue(currentValues[entry.field])}</Trans>
                    </span>
                  ) : null}
                  <span className="block text-xs break-words text-muted-foreground">
                    {currentValues ? <Trans>New: {newValue}</Trans> : newValue}
                  </span>
                </span>
              </label>
            );
          })}
          {metadataEntries.length === 0 && bodyEntries.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              <Trans>No candidate fields or body sections match this type schema.</Trans>
            </p>
          ) : null}
          {bodyEntries.length > 0 ? (
            <div className="mt-3 border-t pt-3">
              <h4 className="text-xs font-semibold text-muted-foreground uppercase">
                <Trans>Body Sections</Trans>
              </h4>
              <div className="mt-2 flex flex-col gap-2">
                {bodyEntries.map((entry) => {
                  const locked = sectionLocks?.has(entry.key) ?? false;
                  return (
                    <label key={entry.key} className="flex min-w-0 items-start gap-2 text-sm">
                      <input
                        type="checkbox"
                        checked={selectedBodySections.has(entry.key)}
                        onChange={() => toggleBodySection(entry.key)}
                        className="mt-1"
                        disabled={!contentWritable || locked}
                      />
                      <span className="min-w-0">
                        <span className="block font-medium">{entry.heading}</span>
                        <span className="block text-xs text-muted-foreground">
                          {entry.externalField}
                          {sectionModes?.[entry.key] ? (
                            <>
                              {" · "}
                              {sectionModes[entry.key] === "replace"
                                ? t`replaces existing section`
                                : t`adds new section`}
                            </>
                          ) : null}
                        </span>
                        <span className="block text-xs break-words text-muted-foreground">
                          {entry.hasValue ? entry.markdown : t`No value returned`}
                        </span>
                      </span>
                    </label>
                  );
                })}
              </div>
            </div>
          ) : null}
        </div>
      ) : (
        <p className="mt-2 text-sm text-muted-foreground">
          <Trans>Choose a candidate to compare fields.</Trans>
        </p>
      )}
    </div>
  );
});

function formatMetadataValue(value: unknown) {
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}
