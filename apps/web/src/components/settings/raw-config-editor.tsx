import { useEffect, useRef, useState } from "react";
import { SaveIcon } from "lucide-react";
import { useQuery } from "@tanstack/react-query";

import { errorMessage } from "@/api/client";
import { rawSettingsConfigQuery } from "@/api/queries";
import { saveRawSettingsConfig } from "@/api/settings";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Placeholder } from "@/components/ui/placeholder";
import { Textarea } from "@/components/ui/textarea";

type RawConfigEditorProps = {
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
export function RawConfigEditor({ settingsWritable = true, onDirtyChange }: RawConfigEditorProps) {
  const raw = useQuery(rawSettingsConfigQuery());
  const [content, setContent] = useState("");
  // The last saved/loaded text; the editor is "dirty" when `content` differs.
  const [baseline, setBaseline] = useState("");
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string>();
  const [error, setError] = useState<string>();

  // Seed the editable text from the loaded file exactly once, so a background
  // refetch (e.g. on window focus) never discards in-progress edits.
  const seededRef = useRef(false);
  useEffect(() => {
    if (seededRef.current) return;
    if (raw.data === undefined) return;
    seededRef.current = true;
    setContent(raw.data.content);
    setBaseline(raw.data.content);
  }, [raw.data]);

  // Dirty only once the seeded text is actually edited — not on mount, and not
  // after the seed echoes the loaded file back into both `content` and `baseline`.
  const dirty = seededRef.current && content !== baseline;
  useEffect(() => {
    onDirtyChange?.(dirty);
    return () => onDirtyChange?.(false);
  }, [dirty, onDirtyChange]);

  async function save() {
    setSaving(true);
    setError(undefined);
    setMessage(undefined);
    try {
      const response = await saveRawSettingsConfig({ content });
      // Echo back exactly what the server stored, and reset the dirty baseline.
      setContent(response.content);
      setBaseline(response.content);
      setMessage("Saved");
      window.dispatchEvent(new Event("kizunashelf-config-saved"));
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setSaving(false);
    }
  }

  const path = raw.data?.vaultConfigPath;

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h1 className="truncate text-base font-semibold">Edit config.yaml</h1>
          {path ? <p className="mt-1 truncate text-xs text-muted-foreground">{path}</p> : null}
        </div>
        <div className="flex items-center gap-2">
          {message ? <span className="text-xs text-muted-foreground">{message}</span> : null}
          <Button type="button" onClick={save} disabled={saving || !settingsWritable || raw.isPending}>
            <SaveIcon data-icon="inline-start" />
            {saving ? "Saving" : "Save"}
          </Button>
        </div>
      </header>

      {!settingsWritable ? (
        <Alert>
          Schema editing is disabled on this instance (read-only). Set
          <code className="mx-1">KIZUNASHELF_SETTINGS_WRITABLE=true</code>
          to enable it.
        </Alert>
      ) : null}

      {error ? (
        <Alert className="whitespace-pre-wrap">
          {error}
        </Alert>
      ) : null}

      {raw.isPending ? (
        <Placeholder>Loading</Placeholder>
      ) : (
        <Textarea
          className="min-h-[60vh] font-mono text-xs"
          value={content}
          onChange={(event) => setContent(event.target.value)}
          disabled={!settingsWritable}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
        />
      )}

      <p className="text-xs text-muted-foreground">
        Saved verbatim. The whole file is validated on save — unknown fields and malformed values are
        rejected.
      </p>
    </div>
  );
}
