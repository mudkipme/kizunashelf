import { useState, type ReactNode } from "react";
import { plural } from "@lingui/core/macro";
import { Trans, useLingui } from "@lingui/react/macro";
import { ChevronLeftIcon, ChevronRightIcon, PlusIcon, Trash2Icon } from "lucide-react";

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
import { Separator } from "@/components/ui/separator";
import type {
  EntityTypeConfig,
  ExternalProviderCatalog,
  FieldConfig,
  HomeConfig,
  HomeSectionConfig,
  Language,
} from "@/types/api";

import { PresetPickerDialog } from "./preset-picker";
import { EmptyConfigLine, TextField } from "./settings-controls";
import { fieldConfigSummary } from "./settings-field-descriptors";
import {
  arrayEditor,
  defaultEntityType,
  defaultField,
  defaultHomeSection,
} from "./settings-model";
import {
  EntityTypeForm,
  FieldForm,
  HomeSectionForm,
  TitleLanguagesContext,
} from "./settings-sections";

// ----------------------------------------------------------------------------
// Shared dialog chrome
// ----------------------------------------------------------------------------

/// A scrollable editor dialog with a sticky header (optional back button) and a
/// sticky footer. Drives the draft-isolated section/type/field editors.
function DialogShell({
  open,
  onOpenChange,
  title,
  description,
  onBack,
  footer,
  children,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description: string;
  onBack?: () => void;
  footer: ReactNode;
  children: ReactNode;
}) {
  const { t } = useLingui();
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/*
        These classes override the shared (shadcn) DialogContent without forking
        it: turn the centered card into a flush, scrollable column, and make it a
        full-screen sheet on phones (`max-sm:` resets the base card positioning),
        reverting to a centered `max-w-2xl` card at `sm`+.
      */}
      <DialogContent className="flex flex-col gap-0 overflow-hidden p-0 max-sm:inset-0 max-sm:max-w-none max-sm:translate-x-0 max-sm:translate-y-0 max-sm:rounded-none max-sm:border-0 sm:max-h-[85vh] sm:max-w-2xl">
        <DialogHeader className="space-y-0 border-b px-4 py-3 pr-12 text-left">
          <div className="flex min-w-0 items-center gap-2">
            {onBack ? (
              <Button type="button" variant="ghost" size="icon" className="-ml-2 shrink-0" onClick={onBack} aria-label={t`Back`}>
                <ChevronLeftIcon />
              </Button>
            ) : null}
            <DialogTitle className="min-w-0 truncate">{title}</DialogTitle>
          </div>
          <DialogDescription className="mt-1">{description}</DialogDescription>
        </DialogHeader>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">{children}</div>
        <DialogFooter className="border-t px-4 py-3">{footer}</DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/// A clickable list row used for types, home sections, and fields. The whole row
/// opens the editor; a trailing trash button removes the item.
function EditableRow({
  title,
  badges,
  onEdit,
  onRemove,
  removeLabel,
}: {
  title: string;
  badges: string[];
  onEdit: () => void;
  onRemove: () => void;
  removeLabel: string;
}) {
  return (
    <div className="flex items-center gap-1 rounded-md border pr-1 transition-colors hover:bg-accent">
      <button
        type="button"
        onClick={onEdit}
        className="flex min-w-0 flex-1 items-center gap-2 rounded-md px-3 py-2 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium">{title}</div>
          {badges.length > 0 ? (
            <div className="mt-1 flex flex-wrap gap-1">
              {badges.map((badge, index) => (
                <Badge key={`${badge}-${index}`} variant="outline">
                  {badge}
                </Badge>
              ))}
            </div>
          ) : null}
        </div>
        <ChevronRightIcon className="shrink-0 text-muted-foreground" />
      </button>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="shrink-0"
        onClick={onRemove}
        aria-label={removeLabel}
      >
        <Trash2Icon />
      </Button>
    </div>
  );
}

/// A section header with a title, count, and an "Add" button.
function ListHeader({ title, count, addLabel, onAdd }: { title: string; count: number; addLabel: string; onAdd: () => void }) {
  return (
    <div className="flex items-center justify-between gap-2">
      <h3 className="text-sm font-medium">
        {title} <span className="text-muted-foreground">({count})</span>
      </h3>
      <Button type="button" variant="outline" size="sm" onClick={onAdd}>
        <PlusIcon data-icon="inline-start" />
        {addLabel}
      </Button>
    </div>
  );
}

// ----------------------------------------------------------------------------
// Types
// ----------------------------------------------------------------------------

function typeProviderCount(type: EntityTypeConfig) {
  return new Set([
    ...(type.externalPriority ?? []),
    ...(type.bodySections ?? []).flatMap((section) =>
      section.kind === "external" ? (section.externalFields ?? []).map((field) => field.source) : [],
    ),
    ...type.fields
      .filter((field) => field.fieldType === "externalRef")
      .map((field) => field.externalRef ?? "")
      .filter(Boolean),
  ]).size;
}

export function TypesSection({
  types,
  providerCatalog,
  languages,
  taxonomyBase,
  taxonomyRoot,
  onChange,
}: {
  types: EntityTypeConfig[];
  providerCatalog?: ExternalProviderCatalog;
  languages: Language[];
  taxonomyBase: string;
  taxonomyRoot: string;
  onChange: (types: EntityTypeConfig[]) => void;
}) {
  const { t } = useLingui();
  // `null` = closed; `"new"` = adding; a number = editing that index.
  const [editing, setEditing] = useState<number | "new" | null>(null);
  const [picking, setPicking] = useState(false);
  const initial = editing === "new" ? defaultEntityType() : editing === null ? null : types[editing];
  const typeList = arrayEditor(types, onChange);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-sm font-medium">
          <Trans>Types</Trans> <span className="text-muted-foreground">({types.length})</span>
        </h3>
        <div className="flex items-center gap-2">
          <Button type="button" size="sm" onClick={() => setPicking(true)}>
            <PlusIcon data-icon="inline-start" />
            <Trans>Add built-in type</Trans>
          </Button>
          <Button type="button" variant="outline" size="sm" onClick={() => setEditing("new")}>
            <Trans>Add custom type</Trans>
          </Button>
        </div>
      </div>
      {types.length === 0 ? (
        <EmptyConfigLine>
          <Trans>No entity types configured.</Trans>
        </EmptyConfigLine>
      ) : (
        <div className="flex flex-col gap-2">
          {types.map((type, index) => (
            <EditableRow
              key={index}
              title={type.label || type.id || t`Entity type`}
              badges={[
                plural(type.fields.length, { one: "# field", other: "# fields" }),
                plural(typeProviderCount(type), { one: "# provider", other: "# providers" }),
                ...(type.path ? [type.path] : []),
              ]}
              onEdit={() => setEditing(index)}
              onRemove={() => typeList.remove(index)}
              removeLabel={t`Remove type`}
            />
          ))}
        </div>
      )}

      {initial ? (
        <TypeEditorDialog
          initial={initial}
          providerCatalog={providerCatalog}
          languages={languages}
          taxonomyBase={taxonomyBase}
          taxonomyRoot={taxonomyRoot}
          onClose={() => setEditing(null)}
          onApply={(value) => {
            if (editing === "new") typeList.append(value);
            else typeList.update(editing as number, value);
            setEditing(null);
          }}
        />
      ) : null}

      {picking ? (
        <PresetPickerDialog
          currentTypes={types}
          languages={languages}
          onClose={() => setPicking(false)}
          onApply={(nextTypes) => onChange(nextTypes)}
        />
      ) : null}
    </div>
  );
}

/// Edits a single entity type in its own draft. Fields are edited via an
/// in-dialog drill-down (push/pop view) so the field config — itself complex —
/// gets a full panel instead of being crammed into the type form.
function TypeEditorDialog({
  initial,
  providerCatalog,
  languages,
  taxonomyBase,
  taxonomyRoot,
  onClose,
  onApply,
}: {
  initial: EntityTypeConfig;
  providerCatalog?: ExternalProviderCatalog;
  languages: Language[];
  taxonomyBase: string;
  taxonomyRoot: string;
  onClose: () => void;
  onApply: (value: EntityTypeConfig) => void;
}) {
  const { t } = useLingui();
  const [draft, setDraft] = useState(initial);
  const [fieldIndex, setFieldIndex] = useState<number | null>(null);
  // A field being *added* is held aside as a working copy, not appended to the
  // draft until "Done" — so backing out discards it (there is nothing to
  // "remove" yet). Editing an existing field, by contrast, writes live.
  const [newField, setNewField] = useState<FieldConfig | null>(null);
  const fields = draft.fields ?? [];

  function setFields(next: EntityTypeConfig["fields"]) {
    setDraft((current) => ({ ...current, fields: next }));
  }
  const fieldList = arrayEditor(fields, setFields);

  // Add-field view: a draft field that is only committed on "Done"; the back
  // arrow discards it.
  if (newField) {
    return (
      <DialogShell
        open
        onOpenChange={(next) => !next && onClose()}
        onBack={() => setNewField(null)}
        title={newField.displayName || newField.field || t`New field`}
        description={t`Configure the new field, then choose Done to add it. Going back discards it.`}
        footer={
          <Button
            type="button"
            onClick={() => {
              fieldList.append(newField);
              setNewField(null);
            }}
          >
            <Trans>Done</Trans>
          </Button>
        }
      >
        <TitleLanguagesContext.Provider value={languages}>
          <FieldForm providerCatalog={providerCatalog} field={newField} onChange={setNewField} />
        </TitleLanguagesContext.Provider>
      </DialogShell>
    );
  }

  // Edit-field drill-down view: edits write straight into the type draft, so the
  // type-level Cancel still discards them.
  if (fieldIndex !== null && fields[fieldIndex]) {
    const field = fields[fieldIndex];
    return (
      <DialogShell
        open
        onOpenChange={(next) => !next && onClose()}
        onBack={() => setFieldIndex(null)}
        title={field.displayName || field.field || t`Field`}
        description={t`Configure how this field is read, derived, and displayed.`}
        footer={
          <>
            <Button
              type="button"
              variant="ghost"
              className="text-destructive sm:mr-auto"
              onClick={() => {
                fieldList.remove(fieldIndex);
                setFieldIndex(null);
              }}
            >
              <Trash2Icon data-icon="inline-start" />
              <Trans>Remove field</Trans>
            </Button>
            <Button type="button" onClick={() => setFieldIndex(null)}>
              <Trans>Done</Trans>
            </Button>
          </>
        }
      >
        <TitleLanguagesContext.Provider value={languages}>
          <FieldForm
            providerCatalog={providerCatalog}
            field={field}
            onChange={(next) => fieldList.update(fieldIndex, next)}
          />
        </TitleLanguagesContext.Provider>
      </DialogShell>
    );
  }

  return (
    <DialogShell
      open
      onOpenChange={(next) => !next && onClose()}
      title={draft.label || draft.id || t`Entity type`}
      description={t`Edit this type's basics, providers, filename, and fields. Changes apply to the draft and save with the rest of the schema.`}
      footer={
        <>
          <Button type="button" variant="outline" onClick={onClose}>
            <Trans>Cancel</Trans>
          </Button>
          <Button type="button" onClick={() => onApply(draft)}>
            <Trans>Apply</Trans>
          </Button>
        </>
      }
    >
      <EntityTypeForm
        config={draft}
        providerCatalog={providerCatalog}
        languages={languages}
        taxonomyBase={taxonomyBase}
        taxonomyRoot={taxonomyRoot}
        onChange={setDraft}
      />
      <Separator className="my-4" />
      <div className="flex flex-col gap-2">
        <ListHeader title={t`Fields`} count={fields.length} addLabel={t`Field`} onAdd={() => setNewField(defaultField())} />
        {fields.length === 0 ? (
          <EmptyConfigLine>
            <Trans>No fields configured.</Trans>
          </EmptyConfigLine>
        ) : (
          <div className="flex flex-col gap-2">
            {fields.map((field, index) => (
              <EditableRow
                key={index}
                title={field.displayName || field.field || t`Field`}
                badges={fieldConfigSummary(field)}
                onEdit={() => setFieldIndex(index)}
                onRemove={() => fieldList.remove(index)}
                removeLabel={t`Remove field`}
              />
            ))}
          </div>
        )}
      </div>
    </DialogShell>
  );
}

// ----------------------------------------------------------------------------
// Home
// ----------------------------------------------------------------------------

export function HomeBlock({
  config,
  types,
  onChange,
}: {
  config: HomeConfig;
  types: EntityTypeConfig[];
  onChange: (config: HomeConfig) => void;
}) {
  const { t } = useLingui();
  const sections = config.sections ?? [];
  const [editing, setEditing] = useState<number | "new" | null>(null);
  const initial =
    editing === "new" ? defaultHomeSection(types[0]?.id) : editing === null ? null : sections[editing];
  const sectionList = arrayEditor(sections, (next) => onChange({ ...config, sections: next }));

  return (
    <div className="flex flex-col gap-3">
      <TextField label={t`Title`} value={config.title ?? ""} onChange={(title) => onChange({ ...config, title })} />
      <ListHeader title={t`Sections`} count={sections.length} addLabel={t`Section`} onAdd={() => setEditing("new")} />
      {sections.length === 0 ? (
        <EmptyConfigLine>
          <Trans>No home sections configured.</Trans>
        </EmptyConfigLine>
      ) : (
        <div className="flex flex-col gap-2">
          {sections.map((section, index) => (
            <EditableRow
              key={index}
              title={section.title || section.id || t`Home section`}
              badges={[
                ...(section.type ? [types.find((type) => type.id === section.type)?.label || section.type] : []),
                ...(section.filters?.length
                  ? [plural(section.filters.length, { one: "# filter", other: "# filters" })]
                  : []),
              ]}
              onEdit={() => setEditing(index)}
              onRemove={() => sectionList.remove(index)}
              removeLabel={t`Remove section`}
            />
          ))}
        </div>
      )}

      {initial ? (
        <HomeSectionDialog
          initial={initial}
          types={types}
          onClose={() => setEditing(null)}
          onApply={(value) => {
            if (editing === "new") sectionList.append(value);
            else sectionList.update(editing as number, value);
            setEditing(null);
          }}
        />
      ) : null}
    </div>
  );
}

function HomeSectionDialog({
  initial,
  types,
  onClose,
  onApply,
}: {
  initial: HomeSectionConfig;
  types: EntityTypeConfig[];
  onClose: () => void;
  onApply: (value: HomeSectionConfig) => void;
}) {
  const { t } = useLingui();
  const [draft, setDraft] = useState(initial);
  return (
    <DialogShell
      open
      onOpenChange={(next) => !next && onClose()}
      title={draft.title || draft.id || t`Home section`}
      description={t`Configure a section shown on the home page: its source type, filters, and ordering.`}
      footer={
        <>
          <Button type="button" variant="outline" onClick={onClose}>
            <Trans>Cancel</Trans>
          </Button>
          <Button type="button" onClick={() => onApply(draft)}>
            <Trans>Apply</Trans>
          </Button>
        </>
      }
    >
      <HomeSectionForm section={draft} types={types} onChange={setDraft} />
    </DialogShell>
  );
}
