//! Step one of the wizard: pick a source, point it at an account or file, and
//! choose which of its buckets to pull.

import { Trans } from "@lingui/react/macro";
import { useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { ImportSourceCatalogItem } from "@/types/api";


export function ConfigureStep({
  sources,
  sourceId,
  onSelectSource,
  source,
  username,
  onUsername,
  onCsvText,
  canStart,
  starting,
  onStart,
}: {
  sources: ImportSourceCatalogItem[];
  sourceId: string;
  onSelectSource: (id: string) => void;
  source: ImportSourceCatalogItem | undefined;
  username: string;
  onUsername: (value: string) => void;
  onCsvText: (value: string) => void;
  canStart: boolean;
  starting: boolean;
  onStart: () => void;
}) {
  return (
    <section className="flex flex-col gap-4 rounded-md border p-4">
      <div className="flex flex-col gap-2">
        <span className="text-sm font-medium">
          <Trans>Source</Trans>
        </span>
        <ul className="grid gap-2 sm:grid-cols-2">
          {sources.map((item) => {
            const selected = item.id === sourceId;
            const disabled = !item.available;
            return (
              <li key={item.id}>
                <button
                  type="button"
                  onClick={() => onSelectSource(item.id)}
                  disabled={disabled}
                  className={[
                    "flex w-full flex-col items-start gap-0.5 rounded-md border p-3 text-left transition-colors",
                    selected ? "border-primary ring-1 ring-primary" : "hover:bg-accent",
                    disabled ? "opacity-60" : "",
                  ].join(" ")}
                >
                  <span className="font-medium">{item.label}</span>
                  <span className="text-xs text-muted-foreground">
                    {item.input === "csv" ? <Trans>CSV export</Trans> : <Trans>Public profile</Trans>}
                  </span>
                  {disabled && item.unavailableReason ? (
                    <span className="mt-0.5 text-xs text-amber-700 dark:text-amber-400">
                      {item.unavailableReason}
                    </span>
                  ) : null}
                </button>
              </li>
            );
          })}
        </ul>
      </div>

      {source ? (
        <div className="flex flex-col gap-2">
          {source.input === "csv" ? (
            <CsvFileInput
              key={source.id}
              label={source.inputLabel}
              onCsvText={onCsvText}
            />
          ) : (
            <label className="flex max-w-sm flex-col gap-1 text-sm font-medium">
              {source.inputLabel}
              <Input
                value={username}
                onChange={(event) => onUsername(event.target.value)}
                placeholder={source.inputLabel}
                autoFocus
              />
            </label>
          )}
          <p className="text-xs text-muted-foreground">
            {source.input === "csv" ? (
              <Trans>The file is read in your browser. Nothing is written until you review the plan.</Trans>
            ) : (
              <Trans>Only public profiles are supported. Nothing is written until you review the plan.</Trans>
            )}
          </p>
          <div>
            <Button onClick={onStart} disabled={!canStart || starting}>
              {starting ? <Trans>Starting…</Trans> : <Trans>Fetch & plan</Trans>}
            </Button>
          </div>
        </div>
      ) : null}
    </section>
  );
}

function CsvFileInput({ label, onCsvText }: { label: string; onCsvText: (value: string) => void }) {
  const [readFailed, setReadFailed] = useState(false);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  async function handleFile(fileInput: HTMLInputElement) {
    const file = fileInput.files?.[0];
    setReadFailed(false);
    onCsvText("");
    if (!file) return;

    try {
      const text = await file.text();
      if (mountedRef.current && fileInput.files?.[0] === file) onCsvText(text);
    } catch {
      if (mountedRef.current && fileInput.files?.[0] === file) setReadFailed(true);
    }
  }

  return (
    <label className="flex max-w-lg flex-col gap-1 text-sm font-medium">
      {label}
      <Input
        type="file"
        accept=".csv,text/csv"
        aria-invalid={readFailed || undefined}
        onChange={(event) => void handleFile(event.currentTarget)}
      />
      {readFailed ? (
        <span className="text-xs font-normal text-destructive" role="alert">
          <Trans>Could not read this CSV file.</Trans>
        </span>
      ) : null}
    </label>
  );
}
