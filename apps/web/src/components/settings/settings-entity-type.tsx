//! The editor for one entity type: its identity and paths, the per-type log
//! block, the external-provider priority, and the body sections written into a
//! new note.

import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { ArrowDownIcon, ArrowUpIcon, PlusIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { externalSourceLabel } from "@/lib/external-metadata";
import type {
  BodySection,
  BodySectionKind,
  EntityTypeConfig,
  ExternalProviderCatalog,
  FieldConfig,
  Language,
} from "@/types/api";

import { EmojiField } from "./emoji-field";
import {
  EmptyConfigLine,
  Field,
  IconButton,
  OptionalToggle,
  PathField,
  TextField,
  UnknownValueOption,
} from "./settings-controls";
import { ExternalFieldMappingsEditor } from "./settings-external-mappings";
import { arrayEditor, externalRefProviderPriority } from "./settings-model";
import { ConfigSubsection, LanguageSelect } from "./settings-shared";

/// The basics / providers / filename portion of an entity type — everything
/// except its fields list, which is edited via a drill-down in the type dialog.
export function EntityTypeForm({
  config,
  providerCatalog,
  languages,
  taxonomyBase,
  taxonomyRoot,
  onChange,
}: {
  config: EntityTypeConfig;
  providerCatalog?: ExternalProviderCatalog;
  languages: Language[];
  /** Absolute taxonomy dir (desktop "Browse" base). */
  taxonomyBase: string;
  /** Vault-relative taxonomy root; the type path is relative to it. */
  taxonomyRoot: string;
  onChange: (config: EntityTypeConfig) => void;
}) {
  const { t } = useLingui();
  return (
    <div className="flex flex-col gap-4">
      <ConfigSubsection title={t`Basics`}>
        <div className="grid grid-cols-1 gap-3 lg:grid-cols-4">
          <TextField
            label={t`ID`}
            value={config.id}
            onChange={(id) => onChange({ ...config, id })}
          />
          <TextField
            label={t`Label`}
            value={config.label}
            onChange={(label) => onChange({ ...config, label })}
          />
          <EmojiField
            label={t`Icon`}
            value={config.icon ?? ""}
            onChange={(icon) => onChange({ ...config, icon })}
          />
          <PathField
            label={t`Path`}
            value={config.path}
            base={taxonomyBase}
            suggestionBase={taxonomyRoot}
            onChange={(path) => onChange({ ...config, path })}
          />
        </div>
      </ConfigSubsection>

      <ConfigSubsection title={t`Providers`}>
        <ExternalPriorityEditor
          providerCatalog={providerCatalog}
          fields={config.fields}
          values={config.externalPriority ?? []}
          onChange={(externalPriority) => onChange({ ...config, externalPriority })}
        />
        <BodySectionsEditor
          providerCatalog={providerCatalog}
          values={config.bodySections ?? []}
          onChange={(bodySections) => onChange({ ...config, bodySections })}
        />
      </ConfigSubsection>

      <ConfigSubsection title={t`Filename`}>
        <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
          <Field label={t`Filename title language`}>
            <LanguageSelect
              value={config.filename?.titleLanguage ?? ""}
              languages={languages}
              onChange={(titleLanguage) =>
                onChange({
                  ...config,
                  filename: {
                    titleLanguage: titleLanguage || undefined,
                    titleRole: config.filename?.titleRole ?? undefined,
                  },
                })
              }
            />
          </Field>
          <Field label={t`Filename title — used as`}>
            <Select
              value={config.filename?.titleRole ?? ""}
              onChange={(event) =>
                onChange({
                  ...config,
                  filename: {
                    titleLanguage: config.filename?.titleLanguage,
                    titleRole: event.target.value === "original" ? "original" : undefined,
                  },
                })
              }
              className="w-full"
            >
              <option value="">{t`None`}</option>
              <option value="original">{t`Original (filename is the title)`}</option>
            </Select>
          </Field>
        </div>
      </ConfigSubsection>

      <ConfigSubsection title={t`Daily-note logging`}>
        <TypeLogEditor config={config} onChange={onChange} />
      </ConfigSubsection>
    </div>
  );
}

/// The per-type daily-note logging block. The toggle is the opt-in: with it off
/// the type isn't loggable (no `log` block); on, it logs under the daily-notes
/// section (or a per-type override) using this line format. `{title}` is always
/// rendered as a wikilink, so the format just carries the tag.
function TypeLogEditor({
  config,
  onChange,
}: {
  config: EntityTypeConfig;
  onChange: (config: EntityTypeConfig) => void;
}) {
  const { t } = useLingui();
  const log = config.log ?? null;
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-xs font-medium text-muted-foreground">
            <Trans>Log to daily note</Trans>
          </div>
          <div className="text-xs text-muted-foreground">
            <Trans>
              When on, checking an episode or the Log button writes a line to the daily note. The
              title is auto-linked as a wikilink — just add your tag to the line format.
            </Trans>
          </div>
        </div>
        <OptionalToggle
          enabled={Boolean(log)}
          onEnable={() => onChange({ ...config, log: { section: "", lineFormat: "" } })}
          onDisable={() => onChange({ ...config, log: null })}
        />
      </div>
      {log ? (
        <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
          <TextField
            label={t`Line format`}
            value={log.lineFormat ?? ""}
            placeholder="- {title} {note} #Tag"
            onChange={(lineFormat) => onChange({ ...config, log: { ...log, lineFormat } })}
          />
          <TextField
            label={t`Section override`}
            value={log.section ?? ""}
            placeholder={t`(daily-notes default)`}
            onChange={(section) => onChange({ ...config, log: { ...log, section } })}
          />
        </div>
      ) : null}
    </div>
  );
}

function ExternalPriorityEditor({
  providerCatalog,
  fields,
  values,
  onChange,
}: {
  providerCatalog?: ExternalProviderCatalog;
  fields: FieldConfig[];
  values: string[];
  onChange: (values: string[]) => void;
}) {
  const { t } = useLingui();
  const ordered = externalRefProviderPriority(fields, values);

  function move(index: number, offset: -1 | 1) {
    const destination = index + offset;
    if (destination < 0 || destination >= ordered.length) return;
    const next = [...ordered];
    [next[index], next[destination]] = [next[destination], next[index]];
    onChange(next);
  }

  return (
    <div className="flex flex-col gap-2">
      <span className="text-xs font-medium text-muted-foreground">
        <Trans>Provider priority</Trans>
      </span>
      <div className="flex flex-col gap-2">
        {ordered.map((source, index) => {
          const label = externalSourceLabel(providerCatalog, source);
          return (
            <div key={source} className="flex items-center gap-2 rounded-md border px-3 py-2">
              <span className="min-w-0 flex-1 truncate text-sm font-medium">{label}</span>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                disabled={index === 0}
                aria-label={t`Move ${label} up`}
                onClick={() => move(index, -1)}
              >
                <ArrowUpIcon />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                disabled={index === ordered.length - 1}
                aria-label={t`Move ${label} down`}
                onClick={() => move(index, 1)}
              >
                <ArrowDownIcon />
              </Button>
            </div>
          );
        })}
        {ordered.length === 0 ? (
          <EmptyConfigLine>
            <Trans>Add an External ref field with a provider to configure priority.</Trans>
          </EmptyConfigLine>
        ) : null}
      </div>
    </div>
  );
}

type BodySectionTracking = NonNullable<BodySection["tracking"]>;

const EPISODE_TRACKING_OPTIONS: { value: BodySectionTracking; label: MessageDescriptor }[] = [
  { value: "checklist", label: msg`Checklist (per-item checkboxes)` },
  { value: "none", label: msg`None (plain list)` },
];

/// Reshapes a section when its `kind` changes, dropping the now-irrelevant
/// payload so the saved config doesn't carry stale fields from the other kind.
function changeBodySectionKind(section: BodySection, kind: BodySectionKind): BodySection {
  if (kind === "episodes") {
    return { heading: section.heading, kind, tracking: section.tracking ?? "checklist" };
  }
  return { heading: section.heading, kind, externalFields: section.externalFields ?? [] };
}

function BodySectionsEditor({
  providerCatalog,
  values,
  onChange,
}: {
  providerCatalog?: ExternalProviderCatalog;
  values: BodySection[];
  onChange: (values: BodySection[]) => void;
}) {
  const { t } = useLingui();
  const list = arrayEditor(values, onChange);
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs font-medium text-muted-foreground">
          <Trans>Markdown body sections</Trans>
        </span>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => list.append({ heading: t`Summary`, kind: "external", externalFields: [] })}
        >
          <PlusIcon data-icon="inline-start" />
          <Trans>Section</Trans>
        </Button>
      </div>
      <div className="flex flex-col gap-2">
        {values.map((section, index) => (
          <BodySectionEditor
            // Index, not heading: the heading is editable; keying on it would
            // remount and drop focus on each keystroke.
            key={index}
            providerCatalog={providerCatalog}
            section={section}
            onChange={(next) => list.update(index, next)}
            onRemove={() => list.remove(index)}
          />
        ))}
        {values.length === 0 ? (
          <EmptyConfigLine>
            <Trans>No markdown body sections.</Trans>
          </EmptyConfigLine>
        ) : null}
      </div>
    </div>
  );
}

function BodySectionEditor({
  providerCatalog,
  section,
  onChange,
  onRemove,
}: {
  providerCatalog?: ExternalProviderCatalog;
  section: BodySection;
  onChange: (section: BodySection) => void;
  onRemove: () => void;
}) {
  const { t, i18n } = useLingui();
  return (
    <div className="flex flex-col gap-3 rounded-md border border-dashed p-3">
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
        <TextField
          label={t`Heading`}
          value={section.heading}
          onChange={(heading) => onChange({ ...section, heading })}
        />
        <Field label={t`Kind`}>
          <Select
            value={section.kind}
            onChange={(event) =>
              onChange(changeBodySectionKind(section, event.target.value as BodySectionKind))
            }
            className="w-full"
          >
            <option value="external">{t`External metadata`}</option>
            <option value="episodes">{t`Item list`}</option>
            <UnknownValueOption value={section.kind} known={["external", "episodes"]} />
          </Select>
        </Field>
        <div className="flex items-end">
          <IconButton label={t`Remove body section`} onClick={onRemove} />
        </div>
      </div>
      {section.kind === "episodes" ? (
        <Field label={t`Tracking`}>
          <Select
            value={section.tracking ?? "checklist"}
            onChange={(event) =>
              onChange({ ...section, tracking: event.target.value as BodySectionTracking })
            }
            className="w-full"
          >
            {EPISODE_TRACKING_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {i18n._(option.label)}
              </option>
            ))}
            <UnknownValueOption
              value={section.tracking ?? "checklist"}
              known={EPISODE_TRACKING_OPTIONS.map((option) => option.value)}
            />
          </Select>
        </Field>
      ) : (
        <ExternalFieldMappingsEditor
          providerCatalog={providerCatalog}
          values={section.externalFields ?? []}
          onChange={(externalFields) => onChange({ ...section, externalFields })}
        />
      )}
    </div>
  );
}
