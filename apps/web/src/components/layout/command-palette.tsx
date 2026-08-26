//! The command palette: one field that reaches every destination, every entity
//! and every global action.
//!
//! It is the app's answer to "where is that?" on all three runtimes, which is
//! why the navigation accelerators it lists (`mod+1`…`mod+9`) are shown only
//! where they are actually bound — the palette itself is the portable path.
//!
//! The listbox is hand-rolled rather than built on the shared `Combobox`. That
//! primitive is a popup anchored to a field, and its filtering owns the item
//! list; here the list is four sources at once, one of them asynchronous, and
//! the keyboard has to move across group boundaries as if they were not there.

import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { Trans, useLingui } from "@lingui/react/macro";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import {
  FilePlus2Icon,
  type LucideIcon,
  RefreshCwIcon,
  SearchIcon,
  TablePropertiesIcon,
} from "lucide-react";

import { entitiesQuery } from "@/api/queries";
import { EntityCover } from "@/components/assets/entity-cover";
import { EntityTitle } from "@/components/entities/entity-title";
import { navDestinations } from "@/components/layout/nav-destinations";
import { Badge } from "@/components/ui/badge";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { useDebouncedValue } from "@/hooks/use-debounce";
import { useRescanLibrary } from "@/hooks/use-rescan-library";
import { allTypes, relevanceSort } from "@/lib/constants";
import { isDesktopRuntime } from "@/lib/desktop";
import { useTitleLanguage } from "@/lib/language";
import { destinationChord, formatChord } from "@/lib/shortcuts";
import { entityTitle } from "@/lib/title-language";
import { cn } from "@/lib/utils";
import type { EntitySummary, StatsResponse } from "@/types/api";

/// How many entity matches the palette previews. Small on purpose: the palette
/// is a jump affordance, and the library page is one Enter away for the rest.
const ENTITY_LIMIT = 6;

type PaletteItem = {
  id: string;
  label: string;
  /// Right-aligned accelerator, shown only where the chord is bound.
  chord?: string;
  icon?: LucideIcon;
  emoji?: string | null;
  entity?: EntitySummary;
  run: () => void;
};

type PaletteGroup = { id: string; label: string; items: PaletteItem[] };

export function CommandPalette({
  open,
  onOpenChange,
  stats,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  stats?: StatsResponse;
}) {
  const { t, i18n } = useLingui();
  const navigate = useNavigate();
  const language = useTitleLanguage();
  const rescan = useRescanLibrary();
  const listId = useId();

  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  const trimmed = query.trim();
  const debouncedQuery = useDebouncedValue(trimmed, 200);
  const entities = useQuery({
    ...entitiesQuery({
      type: allTypes,
      page: 1,
      pageSize: ENTITY_LIMIT,
      sort: relevanceSort,
      titleLanguage: language,
      q: debouncedQuery,
    }),
    enabled: open && debouncedQuery.length > 0,
  });

  // Every reopen starts from a blank field: a palette that remembers the last
  // thing typed makes the second use slower, not faster.
  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActiveIndex(0);
  }, [open]);

  const go = useCallback(
    (to: string) => {
      onOpenChange(false);
      navigate(to);
    },
    [navigate, onOpenChange],
  );

  const destinationItems = useMemo<PaletteItem[]>(
    () =>
      navDestinations.map((destination, index) => {
        // The digit is the destination's position in the canonical list, not
        // its position here — the two differ once the vault's own types are
        // interleaved below.
        const chord = destinationChord(index);
        return {
          id: `go:${destination.to}`,
          label: i18n._(destination.label),
          icon: destination.icon,
          // The digit accelerators exist in the desktop shell only, so showing
          // them in a browser would be advertising a key that does nothing.
          chord: isDesktopRuntime() && chord ? formatChord(chord) : undefined,
          run: () => go(destination.to),
        };
      }),
    [i18n, go],
  );

  const typeItems = useMemo<PaletteItem[]>(
    () =>
      (stats?.byType ?? []).map((type) => ({
        id: `type:${type.id}`,
        label: type.label,
        emoji: type.icon,
        // A type without its own emoji still needs a glyph, or its row hangs
        // left of every other row in the group.
        icon: type.icon ? undefined : TablePropertiesIcon,
        run: () => go(`/library?type=${encodeURIComponent(type.id)}`),
      })),
    [stats?.byType, go],
  );

  const actionItems = useMemo<PaletteItem[]>(
    () => [
      {
        id: "action:new",
        label: t`New entity`,
        icon: FilePlus2Icon,
        run: () => go("/entities/new"),
      },
      {
        id: "action:rescan",
        label: t`Rescan vault`,
        icon: RefreshCwIcon,
        run: () => {
          onOpenChange(false);
          rescan.mutate();
        },
      },
    ],
    [t, go, onOpenChange, rescan],
  );

  const entityItems = useMemo<PaletteItem[]>(
    () =>
      (entities.data?.items ?? []).map((entity) => ({
        id: `entity:${entity.id}`,
        label: entityTitle(entity, language),
        entity,
        run: () => go(`/entities/${encodeURIComponent(entity.id)}`),
      })),
    [entities.data, language, go],
  );

  const groups = useMemo<PaletteGroup[]>(() => {
    const match = (item: PaletteItem) => item.label.toLowerCase().includes(trimmed.toLowerCase());
    // Sidebar order: the fixed top group, the vault's own types, then the
    // bottom group — so the palette reads as the list the user already knows
    // the shape of.
    const navigation = [
      ...destinationItems.filter((_, index) => navDestinations[index].group === "primary"),
      ...typeItems,
      ...destinationItems.filter((_, index) => navDestinations[index].group === "secondary"),
    ].filter(match);
    const actions = actionItems.filter(match);
    const built: PaletteGroup[] = [];
    // Entities lead once there is a query: in a library app the title in the
    // user's head is nearly always what they came here to reach.
    if (trimmed && entityItems.length > 0) {
      built.push({ id: "entities", label: t`Entities`, items: entityItems });
    }
    if (navigation.length > 0) built.push({ id: "go", label: t`Go to`, items: navigation });
    if (actions.length > 0) built.push({ id: "actions", label: t`Actions`, items: actions });
    return built;
  }, [trimmed, entityItems, destinationItems, typeItems, actionItems, t]);

  const flat = useMemo(() => groups.flatMap((group) => group.items), [groups]);
  // Clamped rather than reset: entity results arrive after the keystroke that
  // asked for them, and resetting on every list change would drag the highlight
  // back under the user mid-arrow-key.
  const activeItem = flat.length > 0 ? flat[Math.min(activeIndex, flat.length - 1)] : undefined;

  useEffect(() => {
    listRef.current?.querySelector('[data-active="true"]')?.scrollIntoView({ block: "nearest" });
  }, [activeItem]);

  function move(delta: number) {
    if (flat.length === 0) return;
    setActiveIndex((index) => {
      const from = Math.min(index, flat.length - 1);
      // Wrapping matters more here than in a menu: the list is short and the
      // fastest way to the last item is one press of ArrowUp.
      return (from + delta + flat.length) % flat.length;
    });
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.nativeEvent.isComposing) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      move(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      move(-1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      activeItem?.run();
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        aria-describedby={undefined}
        className="top-[12%] max-h-[76vh] translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-xl"
      >
        <DialogTitle className="sr-only">
          <Trans>Command palette</Trans>
        </DialogTitle>
        <div className="flex items-center gap-2 border-b px-3">
          <SearchIcon className="size-4 shrink-0 text-muted-foreground" />
          <input
            autoFocus
            role="combobox"
            aria-expanded
            aria-controls={listId}
            aria-activedescendant={activeItem ? `${listId}-${activeItem.id}` : undefined}
            aria-label={t`Search entities, pages and commands`}
            value={query}
            placeholder={t`Search entities, pages and commands…`}
            onChange={(event) => {
              setQuery(event.target.value);
              setActiveIndex(0);
            }}
            onKeyDown={handleKeyDown}
            className="h-12 w-full bg-transparent text-base outline-none placeholder:text-muted-foreground md:text-sm"
          />
        </div>

        <div
          ref={listRef}
          role="listbox"
          id={listId}
          aria-label={t`Results`}
          className="min-h-0 flex-1 overflow-y-auto overscroll-contain p-1"
        >
          {flat.length === 0 ? (
            <p className="px-3 py-8 text-center text-sm text-muted-foreground">
              <Trans>No matches</Trans>
            </p>
          ) : null}
          {groups.map((group) => (
            <div key={group.id} role="group" aria-labelledby={`${listId}-group-${group.id}`}>
              <div
                id={`${listId}-group-${group.id}`}
                className="px-2 pt-2 pb-1 text-xs text-muted-foreground"
              >
                {group.label}
              </div>
              {group.items.map((item) => (
                <PaletteRow
                  key={item.id}
                  id={`${listId}-${item.id}`}
                  item={item}
                  active={item === activeItem}
                  language={language}
                  onActivate={() => item.run()}
                  onHover={() => setActiveIndex(flat.indexOf(item))}
                />
              ))}
            </div>
          ))}
        </div>

        <div className="app-chrome flex shrink-0 items-center gap-3 border-t px-3 py-2 text-xs text-muted-foreground">
          <span>
            <Trans>↑↓ to move</Trans>
          </span>
          <span>
            <Trans>↵ to open</Trans>
          </span>
          <span>
            <Trans>esc to close</Trans>
          </span>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function PaletteRow({
  id,
  item,
  active,
  language,
  onActivate,
  onHover,
}: {
  id: string;
  item: PaletteItem;
  active: boolean;
  language: string;
  onActivate: () => void;
  onHover: () => void;
}) {
  const Icon = item.icon;
  return (
    <div
      id={id}
      role="option"
      aria-selected={active}
      data-active={active || undefined}
      // Pointer *move*, not enter: a list that scrolls under a resting cursor
      // would otherwise steal the highlight from the keyboard.
      onPointerMove={onHover}
      onClick={onActivate}
      className={cn(
        "flex cursor-default items-center gap-2 rounded-sm px-2 py-1.5 text-sm",
        active && "bg-accent text-accent-foreground",
      )}
    >
      {item.entity ? (
        <>
          <EntityCover entity={item.entity} />
          <EntityTitle
            entity={item.entity}
            language={language}
            className="min-w-0 flex-1 truncate font-medium"
          />
          <Badge variant="outline" className="shrink-0">
            {item.entity.typeLabel}
          </Badge>
        </>
      ) : (
        <>
          {item.emoji ? (
            <span className="flex size-4 shrink-0 items-center justify-center" aria-hidden="true">
              {item.emoji}
            </span>
          ) : Icon ? (
            <Icon className="size-4 shrink-0 text-muted-foreground" />
          ) : null}
          <span className="min-w-0 flex-1 truncate">{item.label}</span>
          {item.chord ? (
            <kbd className="shrink-0 rounded border px-1.5 py-0.5 text-[0.6875rem] text-muted-foreground">
              {item.chord}
            </kbd>
          ) : null}
        </>
      )}
    </div>
  );
}
