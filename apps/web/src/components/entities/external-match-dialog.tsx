import { SearchIcon, WandSparklesIcon } from "lucide-react";

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
import { Select } from "@/components/ui/select";
import {
  externalSourceLabel,
  type ExternalMetadataPreviewEntry,
} from "@/lib/external-metadata";
import { cn } from "@/lib/utils";
import type { ExternalCandidate, ExternalProviderCatalog } from "@/types/api";

type ExternalRefAction = {
  field: string;
  provider: string;
  value: string;
};

type ExternalMatchDialogProps = {
  open: boolean;
  query: string;
  provider: string;
  candidates: ExternalCandidate[];
  selectedCandidate?: ExternalCandidate;
  metadataEntries: ExternalMetadataPreviewEntry[];
  selectedFields: Set<string>;
  providerCatalog?: ExternalProviderCatalog;
  providerOptions: string[];
  externalSearchEnabled: boolean;
  currentValues?: Record<string, unknown>;
  existingExternalRefs?: ExternalRefAction[];
  searching: boolean;
  applying: boolean;
  contentWritable: boolean;
  applyLabel?: string;
  emptyMessage?: string;
  onOpenChange: (open: boolean) => void;
  onQueryChange: (value: string) => void;
  onProviderChange: (value: string) => void;
  onSearch: () => void;
  onRefreshRef?: (provider: string, value: string) => void;
  onChooseCandidate: (candidate: ExternalCandidate) => void;
  onSelectedFieldsChange: (fields: Set<string>) => void;
  onApply: () => void;
};

export function ExternalMatchDialog({
  open,
  query,
  provider,
  candidates,
  selectedCandidate,
  metadataEntries,
  selectedFields,
  providerCatalog,
  providerOptions,
  externalSearchEnabled,
  currentValues,
  existingExternalRefs = [],
  searching,
  applying,
  contentWritable,
  applyLabel = "Apply Selected",
  emptyMessage = "No candidates loaded",
  onOpenChange,
  onQueryChange,
  onProviderChange,
  onSearch,
  onRefreshRef,
  onChooseCandidate,
  onSelectedFieldsChange,
  onApply,
}: ExternalMatchDialogProps) {
  function toggleField(field: string) {
    const next = new Set(selectedFields);
    if (next.has(field)) next.delete(field);
    else next.add(field);
    onSelectedFieldsChange(next);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="top-0 left-0 flex h-dvh max-w-none translate-x-0 translate-y-0 flex-col gap-0 rounded-none border-0 p-0 sm:top-[50%] sm:left-[50%] sm:h-[min(760px,calc(100dvh-2rem))] sm:w-[min(1100px,calc(100vw-2rem))] sm:translate-x-[-50%] sm:translate-y-[-50%] sm:rounded-md sm:border"
      >
        <DialogHeader className="border-b px-4 py-4 pr-12 sm:px-6">
          <DialogTitle>External Match</DialogTitle>
          <DialogDescription>
            Search configured metadata sources and choose which fields to apply.
          </DialogDescription>
        </DialogHeader>

        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4 sm:p-6">
          <div className="flex flex-col gap-2 md:flex-row md:items-end">
            <label className="flex min-w-0 flex-1 flex-col gap-1 text-sm font-medium">
              Search
              <Input value={query} onChange={(event) => onQueryChange(event.target.value)} />
            </label>
            <Select
              className="h-9 md:w-44"
              value={provider}
              onChange={(event) => onProviderChange(event.target.value)}
              aria-label="Provider"
              disabled={!externalSearchEnabled}
            >
              {providerOptions.length === 0 ? <option value="all">No supported sources</option> : null}
              {providerOptions.length > 1 ? <option value="all">All sources</option> : null}
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
              {searching ? "Searching" : "Search"}
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
                  Refresh {externalSourceLabel(providerCatalog, ref.provider)}
                </Button>
              ))}
            </div>
          ) : null}

          <div className="grid min-h-0 gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(300px,380px)]">
            <div className="grid content-start gap-2">
              {candidates.map((candidate) => {
                const selected =
                  selectedCandidate?.provider === candidate.provider &&
                  selectedCandidate.sourceId === candidate.sourceId;
                return (
                  <button
                    key={`${candidate.provider}:${candidate.sourceId}`}
                    type="button"
                    className={cn(
                      "min-w-0 rounded-md border p-3 text-left transition-colors hover:bg-accent",
                      selected && "border-primary bg-accent",
                    )}
                    onClick={() => onChooseCandidate(candidate)}
                  >
                    <div className="flex min-w-0 items-center gap-2">
                      <Badge variant="secondary">{externalSourceLabel(providerCatalog, candidate.provider)}</Badge>
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

            <div className="rounded-md border p-3">
              <h3 className="text-sm font-semibold">Selected Metadata</h3>
              {selectedCandidate ? (
                <div className="mt-3 flex flex-col gap-2">
                  {metadataEntries.map((entry) => (
                    <label key={entry.field} className="flex min-w-0 items-start gap-2 text-sm">
                      <input
                        type="checkbox"
                        checked={selectedFields.has(entry.field)}
                        onChange={() => toggleField(entry.field)}
                        className="mt-1"
                        disabled={!contentWritable || !entry.hasValue}
                      />
                      <span className="min-w-0">
                        <span className="block font-medium">{entry.label}</span>
                        <span className="block text-xs text-muted-foreground">
                          {entry.externalField ?? "external ref"}
                        </span>
                        {currentValues ? (
                          <span className="block break-words text-xs text-muted-foreground">
                            Current: {formatMetadataValue(currentValues[entry.field])}
                          </span>
                        ) : null}
                        <span className="block break-words text-xs text-muted-foreground">
                          {currentValues ? "New: " : ""}
                          {entry.hasValue ? formatMetadataValue(entry.value) : "No value returned"}
                        </span>
                      </span>
                    </label>
                  ))}
                  {metadataEntries.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                      No candidate fields match this type schema.
                    </p>
                  ) : null}
                </div>
              ) : (
                <p className="mt-2 text-sm text-muted-foreground">Choose a candidate to compare fields.</p>
              )}
            </div>
          </div>
        </div>

        <DialogFooter className="border-t p-4 sm:px-6">
          <Button
            type="button"
            onClick={onApply}
            disabled={!contentWritable || applying || !selectedCandidate || selectedFields.size === 0}
          >
            <WandSparklesIcon data-icon="inline-start" />
            {applying ? "Applying" : applyLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function formatMetadataValue(value: unknown) {
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}
