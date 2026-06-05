import type { FormEvent, ReactNode } from "react";
import { useEffect, useMemo, useState } from "react";
import { getStats } from "@kizunashelf/api-contract";
import {
  ArrowLeftIcon,
  BarChart3Icon,
  CalendarDaysIcon,
  ClipboardCheckIcon,
  DatabaseIcon,
  HomeIcon,
  type LucideIcon,
  Link2Icon,
  MenuIcon,
  SearchIcon,
  SettingsIcon,
  TablePropertiesIcon,
  XIcon,
} from "lucide-react";
import { Link, NavLink, useLocation, useNavigate } from "react-router-dom";

import { apiFetch, isAbortError } from "@/api/client";
import { ThemeModeSelect } from "@/components/layout/theme-mode-select";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { allTypes } from "@/lib/constants";
import type { StatsResponse } from "@/types/api";

export function AppFrame({ error, children }: { error?: string; children: ReactNode }) {
  const navigate = useNavigate();
  const location = useLocation();
  const [stats, setStats] = useState<StatsResponse>();
  const [search, setSearch] = useState("");
  const [mobileSidebarOpen, setMobileSidebarOpen] = useState(false);
  const [mobileSearchOpen, setMobileSearchOpen] = useState(false);
  const [canGoBack, setCanGoBack] = useState(false);
  const activeType = useMemo(() => {
    if (location.pathname !== "/library") return "";
    return new URLSearchParams(location.search).get("type") ?? allTypes;
  }, [location.pathname, location.search]);
  const activeTypeLabel = useMemo(() => {
    if (activeType === allTypes) return "library";
    return stats?.byType.find((type) => type.id === activeType)?.label ?? activeType;
  }, [activeType, stats?.byType]);
  const searchPlaceholder =
    location.pathname === "/library"
      ? `Search ${activeTypeLabel}`
      : "Search library";

  useEffect(() => {
    const controller = new AbortController();
    void loadStats(controller.signal);
    return () => controller.abort();
  }, []);

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
    setCanGoBack(hasAppBackStack());
  }, [location.key, location.pathname, location.search]);

  useEffect(() => {
    if (!mobileSidebarOpen) return;
    function closeOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") setMobileSidebarOpen(false);
    }
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [mobileSidebarOpen]);

  async function loadStats(signal: AbortSignal) {
    try {
      const data = await getStats(undefined, { signal }, apiFetch);
      setStats(data);
    } catch (error) {
      if (isAbortError(error)) return;
    }
  }

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
            aria-label="Go back"
            title="Back"
          >
            <ArrowLeftIcon />
          </Button>
        ) : null}
        <Link to="/" className="flex min-w-0 flex-1 items-center gap-3 sm:flex-none">
          <img
            src="/favicon-96x96.png"
            alt=""
            className="size-8 shrink-0 rounded-md"
            aria-hidden="true"
          />
          <span className="min-w-0">
            <span className="block truncate text-sm font-semibold">KizunaShelf</span>
            <span className="block text-xs leading-4 text-muted-foreground">A personal memory graph</span>
          </span>
        </Link>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="md:hidden"
          onClick={() => setMobileSidebarOpen(true)}
          aria-label="Open navigation"
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
          aria-label="Search library"
          aria-expanded={mobileSearchOpen}
        >
          <SearchIcon />
        </Button>
        <div className="ml-auto flex items-center gap-2 sm:ml-0">
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
  return (
    <form onSubmit={onSubmit} className={className}>
      <SearchIcon className="text-muted-foreground" />
      <Input
        value={search}
        onChange={(event) => onSearchChange(event.target.value)}
        placeholder={placeholder}
        className="min-w-0"
        aria-label="Search library"
        autoFocus={autoFocus}
      />
    </form>
  );
}

function hasAppBackStack() {
  return Number(window.history.state?.idx ?? 0) > 0;
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
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 md:hidden">
      <button
        type="button"
        className="absolute inset-0 bg-background/70"
        onClick={onClose}
        aria-label="Close navigation"
      />
      <aside
        role="dialog"
        aria-modal="true"
        aria-label="Navigation"
        className="relative flex h-full w-[min(20rem,calc(100vw-3rem))] flex-col border-r bg-card shadow-lg"
      >
        <header className="flex min-h-14 items-center gap-3 border-b px-3">
          <img
            src="/favicon-96x96.png"
            alt=""
            className="size-8 shrink-0 rounded-md"
            aria-hidden="true"
          />
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-semibold">KizunaShelf</div>
            <div className="truncate text-xs text-muted-foreground">Navigation</div>
          </div>
          <Button type="button" variant="ghost" size="icon" onClick={onClose} aria-label="Close navigation">
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
        <SidebarSectionLabel>Core Views</SidebarSectionLabel>
        <SidebarNavLink to="/" icon={HomeIcon} end onNavigate={onNavigate}>
          Home
        </SidebarNavLink>
        <SidebarNavLink
          to="/library"
          icon={DatabaseIcon}
          active={pathname === "/library" && activeType === allTypes}
          onNavigate={onNavigate}
        >
          Library
        </SidebarNavLink>
        <SidebarNavLink to="/calendar" icon={CalendarDaysIcon} onNavigate={onNavigate}>
          Calendar
        </SidebarNavLink>
        <SidebarNavLink to="/relations" icon={Link2Icon} onNavigate={onNavigate}>
          Relations
        </SidebarNavLink>
      </section>

      <section className="flex flex-col gap-1">
        <SidebarSectionLabel>Taxonomy</SidebarSectionLabel>

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
          <div className="px-2 py-1 text-xs text-muted-foreground">Loading taxonomy</div>
        ) : null}
      </section>

      <section className="mt-auto flex flex-col gap-1">
        <SidebarNavLink to="/statistics" icon={BarChart3Icon} onNavigate={onNavigate}>
          Statistics
        </SidebarNavLink>
        <SidebarNavLink to="/review" icon={ClipboardCheckIcon} onNavigate={onNavigate}>
          Review
        </SidebarNavLink>
        <SidebarNavLink to="/settings" icon={SettingsIcon} onNavigate={onNavigate}>
          Settings
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
