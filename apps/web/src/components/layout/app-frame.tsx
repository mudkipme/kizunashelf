import type { FormEvent, ReactNode } from "react";
import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  ActivityIcon,
  ArrowLeftIcon,
  BarChart3Icon,
  CalendarDaysIcon,
  ClipboardCheckIcon,
  DatabaseIcon,
  DownloadIcon,
  HomeIcon,
  ListIcon,
  type LucideIcon,
  MenuIcon,
  SearchIcon,
  SettingsIcon,
  TablePropertiesIcon,
  XIcon,
} from "lucide-react";
import { Link, NavLink, useLocation, useNavigate } from "react-router-dom";
import { Trans, useLingui } from "@lingui/react/macro";

import { LanguageSelect } from "@/components/layout/language-select";
import { ThemeModeSelect } from "@/components/layout/theme-mode-select";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { statsQuery } from "@/api/queries";
import { cn } from "@/lib/utils";
import { allTypes } from "@/lib/constants";
import type { StatsResponse } from "@/types/api";

export function AppFrame({ error, children }: { error?: string; children: ReactNode }) {
  const { t } = useLingui();
  const navigate = useNavigate();
  const location = useLocation();
  // Drive the sidebar counts from the shared React Query cache so they stay in
  // sync with mutations (which invalidate the `stats` key) instead of going
  // stale behind a one-shot store fetch.
  const stats = useQuery(statsQuery()).data;
  const [search, setSearch] = useState("");
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false);
  const [mobileSearchOpen, setMobileSearchOpen] = useState(false);
  const [canGoBack, setCanGoBack] = useState(false);
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
    if (!mobileSidebarOpen) return;
    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") setMobileSidebarOpen(false);
    }
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [mobileSidebarOpen]);

  function submitSearch(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
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
  }

  function goBack() {
    if (!canGoBack) return;
    navigate(-1);
  }

  return (
    <main className="flex h-dvh min-h-0 flex-col overflow-hidden bg-background pt-[env(safe-area-inset-top)] text-foreground">
      <header className="flex min-h-14 shrink-0 items-center gap-2 border-b bg-card/85 px-3 py-2 sm:gap-3 sm:px-4">
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
        <Link to="/" className="flex min-w-0 flex-1 items-center gap-3 sm:flex-none">
          <AppLogo />
          <span className="min-w-0">
            <span className="block truncate text-sm font-semibold">KizunaShelf</span>
            <span className="hidden text-xs leading-4 text-muted-foreground sm:block">
              <Trans>A personal memory graph</Trans>
            </span>
          </span>
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
        <SearchForm
          search={search}
          onSearchChange={setSearch}
          onSubmit={submitSearch}
          className="ml-auto hidden min-w-0 items-center gap-2 sm:flex sm:max-w-sm"
          placeholder={searchPlaceholder}
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
          <LanguageSelect />
          <ThemeModeSelect />
          <Badge variant="secondary" className="hidden sm:inline-flex">
            v{__APP_VERSION__}
          </Badge>
        </div>
      </header>

      {mobileSearchOpen ? (
        <div className="shrink-0 border-b bg-card/85 px-3 py-2 sm:hidden">
          <SearchForm
            search={search}
            onSearchChange={setSearch}
            onSubmit={submitSearch}
            className="flex min-w-0 items-center gap-2"
            placeholder={searchPlaceholder}
            autoFocus
          />
        </div>
      ) : null}

      {error ? (
        <div className="shrink-0 border-b bg-destructive/10 px-4 py-2 text-sm text-destructive">
          {error}
        </div>
      ) : null}

      <div className="grid min-h-0 flex-1 grid-cols-1 overflow-hidden md:grid-cols-[224px_minmax(0,1fr)]">
        <AppSidebar stats={stats} activeType={activeType} pathname={location.pathname} />
        <div className="min-h-0 min-w-0 overflow-auto overscroll-contain">{children}</div>
      </div>
      <MobileSidebar
        open={mobileSidebarOpen}
        stats={stats}
        activeType={activeType}
        pathname={location.pathname}
        onClose={() => setMobileSidebarOpen(false)}
      />
    </main>
  );
}

function SearchForm({
  search,
  onSearchChange,
  onSubmit,
  className,
  placeholder,
  autoFocus = false,
}: {
  search: string;
  onSearchChange: (value: string) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  className?: string;
  placeholder: string;
  autoFocus?: boolean;
}) {
  const { t } = useLingui();
  return (
    <form onSubmit={onSubmit} className={className}>
      <SearchIcon className="text-muted-foreground" />
      <Input
        value={search}
        onChange={(event) => onSearchChange(event.target.value)}
        placeholder={placeholder}
        className="min-w-0"
        aria-label={t`Search library`}
        autoFocus={autoFocus}
      />
    </form>
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
}: {
  stats?: StatsResponse;
  activeType: string;
  pathname: string;
}) {
  return (
    <aside className="hidden min-h-0 border-r bg-card/35 md:block">
      <div className="flex h-full min-h-0 flex-col gap-4 overflow-auto overscroll-contain p-3">
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
        className="relative flex h-full w-[min(20rem,calc(100vw-3rem))] flex-col border-r bg-card shadow-lg"
      >
        <header className="flex min-h-14 items-center gap-3 border-b px-3">
          <AppLogo />
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-semibold">KizunaShelf</div>
            <div className="truncate text-xs text-muted-foreground">
              <Trans>Navigation</Trans>
            </div>
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
  return (
    <>
      <section className="flex flex-col gap-1">
        <SidebarSectionLabel>
          <Trans>Core Views</Trans>
        </SidebarSectionLabel>
        <SidebarNavLink to="/" icon={HomeIcon} end onNavigate={onNavigate}>
          <Trans>Home</Trans>
        </SidebarNavLink>
        <SidebarNavLink
          to="/library"
          icon={DatabaseIcon}
          active={pathname === "/library" && activeType === allTypes}
          onNavigate={onNavigate}
        >
          <Trans>Library</Trans>
        </SidebarNavLink>
        <SidebarNavLink to="/calendar" icon={CalendarDaysIcon} onNavigate={onNavigate}>
          <Trans>Calendar</Trans>
        </SidebarNavLink>
        <SidebarNavLink to="/activity" icon={ActivityIcon} onNavigate={onNavigate}>
          <Trans>Activity</Trans>
        </SidebarNavLink>
        <SidebarNavLink to="/lists" icon={ListIcon} onNavigate={onNavigate}>
          <Trans>Lists</Trans>
        </SidebarNavLink>
      </section>

      <section className="flex flex-col gap-1">
        <SidebarSectionLabel>
          <Trans>Taxonomy</Trans>
        </SidebarSectionLabel>

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

      <section className="mt-auto flex flex-col gap-1">
        <SidebarNavLink to="/entities/import" icon={DownloadIcon} onNavigate={onNavigate}>
          <Trans>Import</Trans>
        </SidebarNavLink>
        <SidebarNavLink to="/statistics" icon={BarChart3Icon} onNavigate={onNavigate}>
          <Trans>Statistics</Trans>
        </SidebarNavLink>
        <SidebarNavLink to="/review" icon={ClipboardCheckIcon} onNavigate={onNavigate}>
          <Trans>Review</Trans>
        </SidebarNavLink>
        <SidebarNavLink to="/settings" icon={SettingsIcon} onNavigate={onNavigate}>
          <Trans>Settings</Trans>
        </SidebarNavLink>
      </section>
    </>
  );
}

function SidebarSectionLabel({ children }: { children: ReactNode }) {
  return (
    <div className="px-2 pb-1 text-[11px] font-medium uppercase tracking-normal text-muted-foreground">
      {children}
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
