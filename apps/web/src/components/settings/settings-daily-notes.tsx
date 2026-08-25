//! The vault-wide daily-note settings block.

import { Trans, useLingui } from "@lingui/react/macro";

import type { DailyNotesConfig } from "@/types/api";

import {
  StringListEditor,
  TextField,
} from "./settings-controls";

export function DailyNotesEditor({
  config,
  vaultRoot,
  onChange,
}: {
  config: DailyNotesConfig;
  vaultRoot: string;
  onChange: (config: DailyNotesConfig) => void;
}) {
  const { t } = useLingui();
  const log = config.log ?? { section: "", lineFormat: "" };
  return (
    <div className="flex flex-col gap-3">
      <StringListEditor
        label={t`Paths`}
        values={config.paths ?? []}
        placeholder="Daily Notes"
        base={vaultRoot}
        pathItems
        onChange={(paths) => onChange({ ...config, paths })}
      />
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
        <TextField
          label={t`Date format`}
          value={config.dateFormat ?? ""}
          placeholder="YYYY-MM-DD"
          onChange={(dateFormat) => onChange({ ...config, dateFormat })}
        />
        <TextField
          label={t`New-note template`}
          value={config.template ?? ""}
          placeholder="Templates/Daily Note.md"
          onChange={(template) => onChange({ ...config, template })}
        />
      </div>
      <div className="flex flex-col gap-3 rounded-md border border-dashed p-3">
        <div className="text-xs text-muted-foreground">
          <Trans>
            Logging defaults — the heading log lines are written under, and the fallback line format.
            Each type can override these and adds its own tag.
          </Trans>
        </div>
        <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
          <TextField
            label={t`Log section`}
            value={log.section ?? ""}
            placeholder="Log"
            onChange={(section) => onChange({ ...config, log: { ...log, section } })}
          />
          <TextField
            label={t`Default line format`}
            value={log.lineFormat ?? ""}
            placeholder="- {title} {note}"
            onChange={(lineFormat) => onChange({ ...config, log: { ...log, lineFormat } })}
          />
        </div>
      </div>
    </div>
  );
}
