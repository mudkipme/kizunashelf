import type { ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ArrowLeftIcon,
  type LucideIcon,
  MenuIcon,
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
import { ThemeModeSelect } from "@/components/layout/theme-mode-select";
import { Button } from "@/components/ui/button";
import { statsQuery } from "@/api/queries";
import { useAppShortcuts } from "@/hooks/use-app-shortcuts";
import { useMacTitlebarInset } from "@/hooks/use-mac-titlebar-inset";
import { useRescanLibrary } from "@/hooks/use-rescan-library";
import { isDesktopRuntime, isMacDesktopRuntime, setWindowTitle } from "@/lib/desktop";
import { cn } from "@/lib/utils";
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
  const [canGoBack, setCanGoBack] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const searchInputRef = useRef<HTMLInputElement>(null);
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
    setCanGoBack(location.pathname !== "/" && hasAppBackStack());
  }, [location.key, location.pathname, location.search]);

  useEffect(() => {
    if (!desktop) return;
    const destination = destinationForPath(location.pathname);
    const label = destination ? i18n._(destination.label) : null;
    void setWindowTitle(label ? `${label} — KizunaShelf` : "KizunaShelf");
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

  function goBack() {
    if (!canGoBack) return;
    navigate(-1);
  }

  const headerBar = (
    <header
      data-tauri-drag-region={macDesktop || undefined}
      className="app-chrome flex min-h-14 shrink-0 items-center gap-2 border-b bg-chrome px-3 py-2 sm:gap-3 sm:px-4"
    >
      {canGoBack ? (
        <Button
          type="button"
          variant="ghost"
          size="icon"
          onClick={goBack}
          aria-label={t`Go back`}
          title={t`Back`}
        >
          <ArrowLeftIcon />
        </Button>
      ) : null}
      {/* The wordmark is redundant only where a native title bar already
          says "KizunaShelf" (Windows/Linux desktop); the macOS overlay hides
          the native title, so there the header keeps the full brand. */}
      <Link
        to="/"
        className="flex min-w-0 flex-1 items-center gap-3 sm:flex-none"
        aria-label={desktop && !macDesktop ? "KizunaShelf" : undefined}
        title={desktop && !macDesktop ? "KizunaShelf" : undefined}
      >
        <AppLogo />
        {desktop && !macDesktop ? null : (
          <span className="min-w-0">
            <span className="block truncate text-sm font-semibold">KizunaShelf</span>
            <span className="hidden text-xs leading-4 text-muted-foreground sm:block">
              <Trans>A shelf for everything you love</Trans>
            </span>
          </span>
        )}
      </Link>
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="md:hidden"
        onClick={() => setMobileSidebarOpen(true)}
        aria-label={t`Open navigation`}
        aria-expanded={mobileSidebarOpen}
      >
        <MenuIcon />
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
      stats={stats}
      activeType={activeType}
      pathname={location.pathname}
      onClose={() => setMobileSidebarOpen(false)}
    />
  );

  // macOS overlay title bar: the sidebar owns the top-left corner, so the
  // native traffic lights sit (at their default position) over an empty,
  // draggable strip above it instead of crowding the header's leading
  // controls — the Finder/Obsidian arrangement. The header spans only the
  // content column. Other platforms keep the full-width header under their
  // native title bar.
  if (macDesktop) {
    return (
      <main className="grid h-dvh min-h-0 grid-cols-1 overflow-hidden bg-background text-foreground md:grid-cols-[224px_minmax(0,1fr)]">
        <AppSidebar
          stats={stats}
          activeType={activeType}
          pathname={location.pathname}
          titlebarInset={macTitlebarInset}
        />
        <div className="flex min-h-0 min-w-0 flex-col overflow-hidden">
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

  return (
    <main className="flex h-dvh min-h-0 flex-col overflow-hidden bg-background pt-[env(safe-area-inset-top)] text-foreground">
      {headerBar}
      {mobileSearchBar}
      {errorBar}
      <div className="grid min-h-0 flex-1 grid-cols-1 overflow-hidden md:grid-cols-[224px_minmax(0,1fr)]">
        <AppSidebar stats={stats} activeType={activeType} pathname={location.pathname} />
        <div className="min-h-0 min-w-0 overflow-auto overscroll-contain">{children}</div>
      </div>
      {mobileSidebar}
      {palette}
    </main>
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

function hasAppBackStack() {
  return Number(window.history.state?.idx ?? 0) > 0;
}

function AppLogo() {
  return (
    <span className="relative size-8 shrink-0 overflow-hidden rounded-md" aria-hidden="true">
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
}: {
  stats?: StatsResponse;
  activeType: string;
  pathname: string;
  /** Reserve a draggable strip at the top for the macOS traffic lights
   * (overlay title bar); collapses in fullscreen, where macOS hides them. */
  titlebarInset?: boolean;
}) {
  return (
    <aside className="app-chrome hidden min-h-0 border-r bg-chrome md:flex md:flex-col">
      {titlebarInset ? <div data-tauri-drag-region className="h-9 shrink-0" /> : null}
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto overscroll-contain p-3">
        <SidebarContent stats={stats} activeType={activeType} pathname={pathname} />
      </div>
    </aside>
  );
}

function MobileSidebar({
  open,
  stats,
  activeType,
  pathname,
  onClose,
}: {
  open: boolean;
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
        <header className="flex min-h-14 items-center gap-3 border-b px-3">
          <AppLogo />
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-semibold">KizunaShelf</div>
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
        {!stats ? (
          <div className="px-2 py-1 text-xs text-muted-foreground">
            <Trans>Loading taxonomy</Trans>
          </div>
        ) : null}
      </section>

      <section className="mt-auto flex flex-col gap-1">{destinations("secondary")}</section>
    </>
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
          "flex h-8 min-w-0 items-center gap-2 rounded-md px-2 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
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
