import type { ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ArrowLeftIcon,
  ArrowRightIcon,
  type LucideIcon,
  PanelLeftCloseIcon,
  PanelLeftOpenIcon,
  RefreshCwIcon,
  SearchIcon,
  TablePropertiesIcon,
  XIcon,
} from "lucide-react";
import { Link, NavLink, useLocation, useNavigate } from "react-router-dom";
import { Trans, useLingui } from "@lingui/react/macro";

import { CommandPalette } from "@/components/layout/command-palette";
import { HeaderSearch } from "@/components/layout/header-search";
import { LanguageSelect } from "@/components/layout/language-select";
import { destinationForPath, navDestinations } from "@/components/layout/nav-destinations";
import { SidebarResizer } from "@/components/layout/sidebar-resizer";
import { ThemeModeSelect } from "@/components/layout/theme-mode-select";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { statsQuery } from "@/api/queries";
import { useAppShortcuts } from "@/hooks/use-app-shortcuts";
import { useHistoryPosition } from "@/hooks/use-history-position";
import { useMacTitlebarInset } from "@/hooks/use-mac-titlebar-inset";
import { useRescanLibrary } from "@/hooks/use-rescan-library";
import { isDesktopRuntime, isMacDesktopRuntime, setWindowTitle } from "@/lib/desktop";
import { backChord, formatChord, forwardChord, sidebarChord } from "@/lib/shortcuts";
import { cn } from "@/lib/utils";
import { useSidebarStore } from "@/lib/sidebar";
import { allTypes } from "@/lib/constants";
import type { StatsResponse } from "@/types/api";

export function AppFrame({ error, children }: { error?: string; children: ReactNode }) {
  const { t, i18n } = useLingui();
  const navigate = useNavigate();
  const location = useLocation();
  // In the desktop shell the native title bar already carries the app identity,
  // so the in-app brand collapses to the icon and the window title becomes
  // contextual instead of repeating "KizunaShelf" twice in the same corner.
  const desktop = isDesktopRuntime();
  const macDesktop = isMacDesktopRuntime();
  const macTitlebarInset = useMacTitlebarInset();
  // Drive the sidebar counts from the shared React Query cache so they stay in
  // sync with mutations (which invalidate the `stats` key) instead of going
  // stale behind a one-shot store fetch.
  const stats = useQuery(statsQuery()).data;
  const [search, setSearch] = useState("");
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false);
  const [mobileSearchOpen, setMobileSearchOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const { canGoBack, canGoForward } = useHistoryPosition();
  const sidebarWidth = useSidebarStore((state) => state.width);
  const sidebarCollapsed = useSidebarStore((state) => state.collapsed);
  const setSidebarWidth = useSidebarStore((state) => state.setWidth);
  const resetSidebarWidth = useSidebarStore((state) => state.reset);
  const toggleSidebar = useSidebarStore((state) => state.toggle);
  // Windows and Linux draw "KizunaShelf" in their own title bar, so an in-app
  // wordmark there would be the second one on screen. macOS hides the native
  // title (`hiddenTitle`) and a browser only puts it in the tab, so both of
  // those need the app to say its own name exactly once.
  const showWordmark = !(desktop && !macDesktop);
  const activeType = useMemo(() => {
    if (location.pathname !== "/library") return "";
    return new URLSearchParams(location.search).get("type") ?? allTypes;
  }, [location.pathname, location.search]);
  const activeTypeLabel = useMemo(() => {
    if (activeType === allTypes) return null;
    return stats?.byType.find((type) => type.id === activeType)?.label ?? activeType;
  }, [activeType, stats?.byType]);
  const searchPlaceholder =
    location.pathname === "/library" && activeTypeLabel
      ? t`Search ${activeTypeLabel}`
      : t`Search library`;

  useEffect(() => {
    if (location.pathname !== "/library") {
      setSearch("");
      return;
    }
    setSearch(new URLSearchParams(location.search).get("q") ?? "");
  }, [location.pathname, location.search]);

  useEffect(() => {
    setMobileSidebarOpen(false);
    setMobileSearchOpen(false);
  }, [location.key, location.pathname, location.search]);

  // With the destination name gone from the page itself, the title bar and the
  // browser tab are what still say where you are — which also covers the case
  // where the sidebar is collapsed.
  useEffect(() => {
    const destination = destinationForPath(location.pathname);
    const label = destination ? i18n._(destination.label) : null;
    const title = label ? `${label} — KizunaShelf` : "KizunaShelf";
    document.title = title;
    if (desktop) void setWindowTitle(title);
  }, [desktop, location.pathname, i18n]);

  // Below `sm` the header search is not rendered at all, so the shortcut opens
  // the sheet that holds it — which autofocuses its own field.
  const focusSearch = useCallback(() => {
    const input = searchInputRef.current;
    if (!input || input.offsetParent === null) {
      setMobileSearchOpen(true);
      return;
    }
    input.focus();
    // Selecting the existing query means the next keystroke replaces it, the
    // way re-invoking find does in a native app.
    input.select();
  }, []);

  useAppShortcuts({
    paletteOpen,
    onTogglePalette: useCallback(() => setPaletteOpen((open) => !open), []),
    onFocusSearch: focusSearch,
    onToggleSidebar: toggleSidebar,
  });

  useEffect(() => {
    if (!mobileSidebarOpen) return;
    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") setMobileSidebarOpen(false);
    }
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [mobileSidebarOpen]);

  const submitSearch = useCallback(() => {
    const params =
      location.pathname === "/library"
        ? new URLSearchParams(location.search)
        : new URLSearchParams();
    const query = search.trim();
    if (query) params.set("q", query);
    else params.delete("q");
    if (!params.get("type")) params.set("type", activeType || allTypes);
    params.set("page", "1");
    setMobileSearchOpen(false);
    navigate(`/library?${params.toString()}`);
  }, [location.pathname, location.search, search, activeType, navigate]);

  const selectSearchEntity = useCallback(
    (id: string) => {
      setMobileSearchOpen(false);
      navigate(`/entities/${encodeURIComponent(id)}`);
    },
    [navigate],
  );

  const headerBar = (
    <header
      data-tauri-drag-region={macDesktop || undefined}
      className="app-chrome flex min-h-(--toolbar-height) shrink-0 items-center gap-2 border-b bg-chrome px-2 py-1.5 sm:gap-3 sm:px-3"
    >
      {/* Reserve the macOS traffic lights' corner. Collapsed, the sidebar is no
          longer there to own the top-left, so the lights land on whatever the
          header puts first.
          A spacer rather than padding on the header itself: `pl-*` loses to the
          `sm:px-*` already on that element, because Tailwind emits breakpoint
          variants after base utilities — so the padding version silently did
          nothing at every width this window can actually be. Being an element
          also lets the strip carry the drag region, which is what the corner
          should do. */}
      {macTitlebarInset && sidebarCollapsed ? (
        <div data-tauri-drag-region aria-hidden="true" className="w-20 shrink-0 self-stretch" />
      ) : null}
      {/* Wide enough for a persistent sidebar: collapse it. Narrower: the
          sidebar is a sheet, so the same corner opens that instead. */}
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="hidden md:inline-flex"
        onClick={toggleSidebar}
        aria-label={sidebarCollapsed ? t`Show sidebar` : t`Hide sidebar`}
        aria-expanded={!sidebarCollapsed}
        title={formatChord(sidebarChord)}
      >
        {sidebarCollapsed ? <PanelLeftOpenIcon /> : <PanelLeftCloseIcon />}
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="md:hidden"
        onClick={() => setMobileSidebarOpen(true)}
        aria-label={t`Open navigation`}
        aria-expanded={mobileSidebarOpen}
      >
        <AppLogo className="size-5" />
      </Button>
      {/* Present at every width: added to a home screen (the manifest declares
          `display: standalone`) there is no browser chrome to go back with, and
          nothing else on the page can move history. */}
      <Button
        type="button"
        variant="ghost"
        size="icon"
        onClick={() => navigate(-1)}
        disabled={!canGoBack}
        aria-label={t`Go back`}
        title={`${t`Back`} ${formatChord(backChord)}`}
      >
        <ArrowLeftIcon />
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        onClick={() => navigate(1)}
        disabled={!canGoForward}
        aria-label={t`Go forward`}
        title={`${t`Forward`} ${formatChord(forwardChord)}`}
      >
        <ArrowRightIcon />
      </Button>
      <HeaderSearch
        search={search}
        onSearchChange={setSearch}
        onSubmit={submitSearch}
        onSelectEntity={selectSearchEntity}
        type={activeType || allTypes}
        className="ml-auto hidden min-w-0 items-center gap-2 sm:flex sm:max-w-sm"
        placeholder={searchPlaceholder}
        inputRef={searchInputRef}
        showShortcutHint
      />
      <Button
        type="button"
        variant={mobileSearchOpen || search.trim() ? "secondary" : "ghost"}
        size="icon"
        className="sm:hidden"
        onClick={() => setMobileSearchOpen((open) => !open)}
        aria-label={t`Search library`}
        aria-expanded={mobileSearchOpen}
      >
        <SearchIcon />
      </Button>
      <div className="ml-auto flex items-center gap-2 sm:ml-0">
        <RescanButton />
        <LanguageSelect />
        <ThemeModeSelect />
      </div>
    </header>
  );

  const mobileSearchBar = mobileSearchOpen ? (
    <div className="shrink-0 border-b bg-chrome px-3 py-2 sm:hidden">
      <HeaderSearch
        search={search}
        onSearchChange={setSearch}
        onSubmit={submitSearch}
        onSelectEntity={selectSearchEntity}
        type={activeType || allTypes}
        className="flex min-w-0 items-center gap-2"
        placeholder={searchPlaceholder}
        autoFocus
      />
    </div>
  ) : null;

  const errorBar = error ? (
    <div className="shrink-0 border-b bg-destructive/10 px-4 py-2 text-sm text-destructive">
      {error}
    </div>
  ) : null;

  const palette = (
    <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} stats={stats} />
  );

  const mobileSidebar = (
    <MobileSidebar
      open={mobileSidebarOpen}
      showWordmark={showWordmark}
      stats={stats}
      activeType={activeType}
      pathname={location.pathname}
      onClose={() => setMobileSidebarOpen(false)}
    />
  );

  // One layout everywhere: the sidebar runs the full height of the window and
  // the header spans only the content column beside it. That arrangement came
  // from macOS — where the overlay title bar leaves the native traffic lights
  // sitting over a draggable strip at the top of the sidebar — but it is the
  // Finder/Obsidian/VS Code shape on every platform, and having one of them
  // rather than two is what keeps the brand from appearing twice.
  return (
    <main className="flex h-dvh min-h-0 overflow-hidden bg-background pt-[env(safe-area-inset-top)] text-foreground">
      {sidebarCollapsed ? null : (
        <AppSidebar
          stats={stats}
          activeType={activeType}
          pathname={location.pathname}
          titlebarInset={macTitlebarInset}
          showWordmark={showWordmark}
          width={sidebarWidth}
          onWidth={setSidebarWidth}
          onResetWidth={resetSidebarWidth}
        />
      )}
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        {headerBar}
        {mobileSearchBar}
        {errorBar}
        <div className="min-h-0 min-w-0 flex-1 overflow-auto overscroll-contain">{children}</div>
      </div>
      {mobileSidebar}
      {palette}
    </main>
  );
}

/**
 * The app's own loading state: the real chrome with an empty content column.
 *
 * Shown while the first settings request is in flight and while a lazy route
 * chunk downloads. Rendering the frame rather than a "Loading" screen means the
 * sidebar and toolbar are already in their final position when the page
 * arrives — only the content column changes — and the sidebar's stats request
 * starts in parallel with the settings one instead of after it.
 */
export function AppShellFallback() {
  return (
    <AppFrame>
      <div className="flex min-h-full items-center justify-center p-8">
        <AppLogo className="size-10 animate-pulse opacity-70" />
      </div>
    </AppFrame>
  );
}

function RescanButton() {
  const { t } = useLingui();
  const rescan = useRescanLibrary();
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon"
      className="hidden sm:inline-flex"
      onClick={() => rescan.mutate()}
      disabled={rescan.isPending}
      aria-label={t`Rescan vault`}
      title={t`Rescan vault`}
    >
      <RefreshCwIcon className={cn(rescan.isPending && "animate-spin")} />
    </Button>
  );
}

function AppLogo({ className }: { className?: string }) {
  return (
    <span
      className={cn("relative size-8 shrink-0 overflow-hidden rounded-md", className)}
      aria-hidden="true"
    >
      <img src="/icon.png" alt="" className="size-full dark:hidden" />
      <img src="/icon-dark.png" alt="" className="hidden size-full dark:block" />
    </span>
  );
}

function AppSidebar({
  stats,
  activeType,
  pathname,
  titlebarInset,
  showWordmark,
  width,
  onWidth,
  onResetWidth,
}: {
  stats?: StatsResponse;
  activeType: string;
  pathname: string;
  /** Reserve a draggable strip at the top for the macOS traffic lights
   * (overlay title bar); collapses in fullscreen, where macOS hides them. */
  titlebarInset?: boolean;
  showWordmark: boolean;
  width: number;
  onWidth: (width: number) => void;
  onResetWidth: () => void;
}) {
  return (
    <aside
      style={{ width }}
      className="app-chrome relative hidden min-h-0 shrink-0 border-r bg-chrome md:flex md:flex-col"
    >
      {titlebarInset ? <div data-tauri-drag-region className="h-9 shrink-0" /> : null}
      {/* Matches the header's height so the two columns start level. No border
          of its own: the sidebar reads as one continuous surface, the way a
          native source list does. */}
      <div className="flex min-h-(--toolbar-height) shrink-0 items-center px-3">
        <Link to="/" className="flex min-w-0 items-center gap-2" aria-label="KizunaShelf">
          <AppLogo />
          {showWordmark ? (
            <span className="truncate text-sm font-semibold">KizunaShelf</span>
          ) : null}
        </Link>
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto overscroll-contain p-3 pt-0">
        <SidebarContent stats={stats} activeType={activeType} pathname={pathname} />
      </div>
      <SidebarResizer width={width} onWidth={onWidth} onReset={onResetWidth} />
    </aside>
  );
}

function MobileSidebar({
  open,
  showWordmark,
  stats,
  activeType,
  pathname,
  onClose,
}: {
  open: boolean;
  showWordmark: boolean;
  stats?: StatsResponse;
  activeType: string;
  pathname: string;
  onClose: () => void;
}) {
  const { t } = useLingui();
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 md:hidden">
      <button
        type="button"
        className="absolute inset-0 bg-background/70"
        onClick={onClose}
        aria-label={t`Close navigation`}
      />
      <aside
        role="dialog"
        aria-modal="true"
        aria-label={t`Navigation`}
        className="app-chrome relative flex h-full w-[min(20rem,calc(100vw-3rem))] flex-col border-r bg-chrome shadow-lg"
      >
        <header className="flex min-h-(--toolbar-height) items-center gap-2 border-b px-3">
          <AppLogo />
          <div className="min-w-0 flex-1">
            {showWordmark ? (
              <span className="block truncate text-sm font-semibold">KizunaShelf</span>
            ) : null}
          </div>
          <Button type="button" variant="ghost" size="icon" onClick={onClose} aria-label={t`Close navigation`}>
            <XIcon />
          </Button>
        </header>
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto p-3">
          <SidebarContent
            stats={stats}
            activeType={activeType}
            pathname={pathname}
            onNavigate={onClose}
          />
        </div>
      </aside>
    </div>
  );
}

function SidebarContent({
  stats,
  activeType,
  pathname,
  onNavigate,
}: {
  stats?: StatsResponse;
  activeType: string;
  pathname: string;
  onNavigate?: () => void;
}) {
  const { i18n } = useLingui();
  const destinations = (group: "primary" | "secondary") =>
    navDestinations
      .filter((destination) => destination.group === group)
      .map((destination) => (
        <SidebarNavLink
          key={destination.to}
          to={destination.to}
          icon={destination.icon}
          end={destination.end}
          // Library is the one destination a route can be "on" without being
          // the active nav item: a type filter belongs to that type's row.
          active={
            destination.to === "/library"
              ? pathname === "/library" && activeType === allTypes
              : undefined
          }
          onNavigate={onNavigate}
        >
          {i18n._(destination.label)}
        </SidebarNavLink>
      ));

  return (
    <>
      <section className="flex flex-col gap-1">{destinations("primary")}</section>

      <section className="flex flex-col gap-1">
        {stats?.byType.map((type) => (
          <SidebarNavLink
            key={type.id}
            to={`/library?type=${encodeURIComponent(type.id)}`}
            icon={type.icon ? undefined : TablePropertiesIcon}
            emoji={type.icon}
            active={activeType === type.id}
            onNavigate={onNavigate}
          >
            <span className="truncate">{type.label}</span>
            <span className="ml-auto tabular-nums text-muted-foreground">{type.count}</span>
          </SidebarNavLink>
        ))}
        {!stats ? <SidebarTypesSkeleton /> : null}
      </section>

      <section className="mt-auto flex flex-col gap-1">{destinations("secondary")}</section>
    </>
  );
}

// Stand-in rows for the vault's types while `stats` loads. Placeholders rather
// than a "loading" line because the types are the tallest part of the sidebar:
// text there would let the sections below it jump once the real rows land.
// Widths vary so the block reads as a list of names, not a progress bar.
const SKELETON_TYPE_WIDTHS = ["w-20", "w-14", "w-24", "w-16"];

function SidebarTypesSkeleton() {
  return (
    <div role="status" className="flex flex-col gap-1">
      <span className="sr-only">
        <Trans>Loading taxonomy</Trans>
      </span>
      {SKELETON_TYPE_WIDTHS.map((width) => (
        <div key={width} className="flex h-(--control-height) items-center gap-2 px-2">
          <Skeleton className="size-4 rounded-sm" />
          <Skeleton className={cn("h-3", width)} />
        </div>
      ))}
    </div>
  );
}

function SidebarNavLink({
  to,
  icon: Icon,
  emoji,
  end,
  active,
  onNavigate,
  children,
}: {
  to: string;
  icon?: LucideIcon;
  emoji?: string | null;
  end?: boolean;
  active?: boolean;
  onNavigate?: () => void;
  children: ReactNode;
}) {
  return (
    <NavLink
      to={to}
      end={end}
      className={({ isActive }) =>
        cn(
          "flex h-(--control-height) min-w-0 items-center gap-2 rounded-md px-2 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
          "[&_svg:not([class*='size-'])]:size-4 [&_svg]:shrink-0",
          (active ?? isActive) && "bg-accent text-foreground",
        )
      }
      onClick={onNavigate}
    >
      {emoji ? (
        <span className="flex size-4 shrink-0 items-center justify-center text-sm leading-none" aria-hidden="true">
          {emoji}
        </span>
      ) : Icon ? (
        <Icon />
      ) : null}
      {children}
    </NavLink>
  );
}
