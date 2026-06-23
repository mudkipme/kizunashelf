import type { ReactNode } from "react";
import { useEffect, useId, useState } from "react";
import { FolderOpenIcon, PlusIcon, Trash2Icon } from "lucide-react";

import { getPathSuggestions } from "@/api/settings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { MultiValueCombobox } from "@/components/ui/multi-value-combobox";
import { isDesktopRuntime, selectDirectory } from "@/lib/desktop";

import { arrayEditor, relativeToBase } from "./settings-model";

/// Renders a fallback `<option>` for a configured value the option list doesn't
/// recognize, so hand-edited config (an unknown type, sort key, field, or title
/// language) stays selectable instead of being silently dropped. Render it last
/// inside a `<Select>`, after the known options. Empty values are skipped.
export function UnknownValueOption({
  value,
  known,
}: {
  value: string;
  known: readonly string[];
}) {
  if (!value || known.includes(value)) return null;
  return <option value={value}>{value}</option>;
}

export function StringListEditor({
  label,
  values,
  placeholder = "field",
  suggestions = [],
  base,
  pathItems = false,
  onChange,
}: {
  label: string;
  values: string[];
  placeholder?: string;
  suggestions?: string[];
  base?: string;
  pathItems?: boolean;
  onChange: (values: string[]) => void;
}) {
  const list = arrayEditor(values, onChange);
  if (!pathItems && suggestions.length > 0) {
    return (
      <div className="flex flex-col gap-2">
        <span className="text-xs font-medium text-muted-foreground">{label}</span>
        <MultiValueCombobox
          values={values}
          options={suggestions.map((suggestion) => ({ value: suggestion }))}
          placeholder={placeholder}
          ariaLabel={label}
          allowCustomValue
          onChange={onChange}
        />
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground">{label}</span>
        <Button type="button" variant="outline" size="sm" onClick={() => list.append("")}>
          <PlusIcon data-icon="inline-start" />
          Add
        </Button>
      </div>
      <div className="flex flex-col gap-2">
        {values.map((value, index) => (
          <div key={index} className="flex items-center gap-2">
            {pathItems ? (
              <PathField
                value={value}
                base={base}
                placeholder={placeholder}
                onChange={(next) => list.update(index, next)}
                hideLabel
              />
            ) : (
              <Input
                value={value}
                placeholder={placeholder}
                onChange={(event) => list.update(index, event.target.value)}
              />
            )}
            <IconButton
              label={`Remove ${label}`}
              onClick={() => list.remove(index)}
            />
          </div>
        ))}
        {values.length === 0 ? <EmptyConfigLine>No values.</EmptyConfigLine> : null}
      </div>
    </div>
  );
}

export function PathField({
  label,
  value,
  base,
  suggestionBase,
  placeholder,
  hideLabel = false,
  onChange,
}: {
  label?: string;
  value: string;
  /** Vault root (desktop "Browse" only), used to relativize a chosen folder. */
  base?: string;
  /**
   * Vault-relative directory the suggestions (and `value`) are relative to —
   * e.g. the taxonomy root for a type's folder path. Omitted = the vault root.
   */
  suggestionBase?: string;
  placeholder?: string;
  hideLabel?: boolean;
  onChange: (value: string) => void;
}) {
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const datalistId = useId();
  const desktop = isDesktopRuntime();

  useEffect(() => {
    const controller = new AbortController();
    const timeout = window.setTimeout(() => {
      // `value` is the prefix relative to `suggestionBase`; suggestions come back
      // in that same coordinate system.
      void getPathSuggestions(value, suggestionBase, { signal: controller.signal }).then(
        (result) => setSuggestions(result.suggestions),
        () => setSuggestions([]),
      );
    }, 120);
    return () => {
      controller.abort();
      window.clearTimeout(timeout);
    };
  }, [suggestionBase, value]);

  async function browse() {
    const selected = await selectDirectory(base || value).catch(() => undefined);
    if (!selected) return;
    onChange(!base ? selected : relativeToBase(selected, base));
  }

  const input = (
    <div className="flex min-w-0 flex-1 items-center gap-2">
      <Input
        value={value}
        placeholder={placeholder}
        list={datalistId}
        onChange={(event) => onChange(event.target.value)}
      />
      <datalist id={datalistId}>
        {suggestions.map((suggestion) => (
          <option key={suggestion} value={suggestion} />
        ))}
      </datalist>
      {desktop ? (
        <Button type="button" variant="outline" size="icon" onClick={browse} aria-label="Select folder">
          <FolderOpenIcon />
        </Button>
      ) : null}
    </div>
  );

  if (hideLabel) return input;
  return <Field label={label ?? "Path"}>{input}</Field>;
}

export function TextField({
  label,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}) {
  return (
    <Field label={label}>
      <Input
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
      />
    </Field>
  );
}

export function NumberField({
  label,
  value,
  onChange,
}: {
  label: string;
  value?: number | null;
  onChange: (value: number | null) => void;
}) {
  return (
    <Field label={label}>
      <Input
        type="number"
        value={value ?? ""}
        onChange={(event) => onChange(event.target.value === "" ? null : Number(event.target.value))}
      />
    </Field>
  );
}

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex min-w-0 flex-col gap-1">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
    </label>
  );
}

export function SettingsSection({
  id,
  title,
  description,
  summary,
  action,
  children,
}: {
  id?: string;
  title: string;
  description?: string;
  summary?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section id={id} className="scroll-mt-4 rounded-md border">
      <header className="flex flex-wrap items-start justify-between gap-3 border-b px-3 py-2">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-sm font-semibold">{title}</h2>
            {summary}
          </div>
          {description ? <p className="mt-1 text-xs text-muted-foreground">{description}</p> : null}
        </div>
        {action}
      </header>
      <div className="flex flex-col gap-4 p-3">{children}</div>
    </section>
  );
}

export function OptionalToggle({
  enabled,
  onEnable,
  onDisable,
}: {
  enabled: boolean;
  onEnable: () => void;
  onDisable: () => void;
}) {
  return (
    <Button type="button" variant="outline" size="sm" onClick={enabled ? onDisable : onEnable}>
      {enabled ? "Disable" : "Enable"}
    </Button>
  );
}

export function IconButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <Button type="button" variant="ghost" size="icon" onClick={onClick} aria-label={label}>
      <Trash2Icon />
    </Button>
  );
}

export function EmptyConfigLine({ children }: { children: ReactNode }) {
  return <div className="rounded-md border border-dashed p-3 text-sm text-muted-foreground">{children}</div>;
}
