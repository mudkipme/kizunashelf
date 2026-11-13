import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { SaveIcon } from "lucide-react";
import { type Ref, useCallback, useEffect, useImperativeHandle, useState } from "react";
import { toast } from "sonner";

import { errorMessage, isConflictError } from "@/api/client";
import { rawSettingsConfigQuery } from "@/api/queries";
import { saveRawSettingsConfig } from "@/api/settings";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Placeholder } from "@/components/ui/placeholder";
import { Textarea } from "@/components/ui/textarea";

import { type SettingsEditorHandle, useServerDraftSync } from "./settings-shared";

type RawConfigEditorProps = {
  ref?: Ref<SettingsEditorHandle>;
  onSavingChange?: (saving: boolean) => void;
  /** When false, the config is read-only (the server enforces it too). */
  settingsWritable?: boolean;
  /** Reports whether the editor holds unsaved edits (differs from the saved file). */
  onDirtyChange?: (dirty: boolean) => void;
};

/**
 * Plain-text editor for `KizunaShelf/config.yaml`. Unlike the structured schema
 * form, it edits the file verbatim (comments + formatting preserved). On save the
 * server strictly validates the YAML — type errors, missing required fields, and
 * any unknown field are rejected — so the on-disk schema can never be corrupted.
 */
export function RawConfigEditor({
  ref,
  onSavingChange,
  settingsWritable = true,
  onDirtyChange,
}: RawConfigEditorProps) {
  const { t } = useLingui();
  const queryClient = useQueryClient();
  const raw = useQuery(rawSettingsConfigQuery());
  const [content, setContent] = useState("");
  // The last saved/loaded text; the editor is "dirty" when `content` differs.
  const [baseline, setBaseline] = useState("");
  const [saving, setSaving] = useState(false);
  // Save failures here are the server's strict YAML validation output — often
  // multi-line (which field, what was wrong). That needs a persistent, full-width
  // surface, so it stays inline rather than becoming an ephemeral toast.
  const [error, setError] = useState<string>();

  // Seed the editable text from the loaded file, ignoring background refetches
  // when nothing changed externally (including our own save echoing back).
  const dirty = content !== baseline;
  const applyRawContent = useCallback((next: string) => {
    setContent(next);
    setBaseline(next);
  }, []);
  const {
    loadedRevisionRef,
    externalChange,
    setExternalChange,
    acceptSave,
    reloadServer,
    observeServer,
  } = useServerDraftSync<string>(applyRawContent);
  useEffect(() => {
    if (raw.data === undefined) return;
    const signature = raw.data.revision ?? raw.data.content;
    observeServer(raw.data.content, signature, raw.data.revision, dirty);
  }, [dirty, observeServer, raw.data]);

  // Dirty only once the seeded text is actually edited — not on mount, and not
  // after the seed echoes the loaded file back into both `content` and `baseline`.
  useEffect(() => {
    onDirtyChange?.(dirty);
    return () => onDirtyChange?.(false);
  }, [dirty, onDirtyChange]);

  async function save() {
    if (saving || externalChange || !settingsWritable || raw.isPending) return { saved: false };
    setSaving(true);
    setError(undefined);
    try {
      const response = await saveRawSettingsConfig({
        content,
        revision: loadedRevisionRef.current,
      });
      // Echo back exactly what the server stored, and reset the dirty baseline.
      acceptSave(response.content, response.revision ?? response.content, response.revision);
      toast.success(t`Settings saved`);
      // The schema every other query derives from just changed.
      void queryClient.invalidateQueries();
      return { saved: true };
    } catch (saveError) {
      if (isConflictError(saveError)) setExternalChange(true);
      setError(errorMessage(saveError));
      return { saved: false, error: saveError };
    } finally {
      setSaving(false);
    }
  }

  useImperativeHandle(ref, () => ({
    save,
    canSave: !saving && !externalChange && settingsWritable && !raw.isPending,
  }));
  useEffect(() => {
    onSavingChange?.(saving);
  }, [saving, onSavingChange]);

  const path = raw.data?.vaultConfigPath;

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h1 className="truncate text-base font-semibold">
            <Trans>Edit config.yaml</Trans>
          </h1>
          {path ? <p className="mt-1 truncate text-xs text-muted-foreground">{path}</p> : null}
        </div>
        <div className="flex items-center gap-2">
          <Button
            type="button"
            onClick={save}
            disabled={saving || externalChange || !settingsWritable || raw.isPending}
          >
            <SaveIcon data-icon="inline-start" />
            {saving ? t`Saving…` : t`Save`}
          </Button>
        </div>
      </header>

      {!settingsWritable ? (
        <Alert>
          <Trans>
            Schema editing is disabled on this instance (read-only). Set
            <code className="mx-1">KIZUNASHELF_SETTINGS_WRITABLE=true</code>
            to enable it.
          </Trans>
        </Alert>
      ) : null}

      {error ? <Alert className="whitespace-pre-wrap">{error}</Alert> : null}

      {externalChange ? (
        <Alert className="flex flex-wrap items-center justify-between gap-3">
          <span className="min-w-0">
            <Trans comment="Warning banner in the raw YAML editor after another app changes config.yaml; the local draft has been preserved">
              config.yaml changed on disk. Your unsaved edits are still here.
            </Trans>
          </span>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => raw.data && reloadServer(raw.data.content, raw.data.revision)}
          >
            <Trans>Reload latest version</Trans>
          </Button>
        </Alert>
      ) : null}

      {raw.isPending ? (
        <Placeholder>
          <Trans>Loading…</Trans>
        </Placeholder>
      ) : (
        <Textarea
          aria-label={t`Edit config.yaml`}
          className="min-h-[60vh] font-mono text-code"
          value={content}
          onChange={(event) => setContent(event.target.value)}
          disabled={saving || !settingsWritable || externalChange}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
        />
      )}

      <p className="text-xs text-muted-foreground">
        <Trans>Changes are validated before saving. Comments and formatting are preserved.</Trans>
      </p>
    </div>
  );
}
