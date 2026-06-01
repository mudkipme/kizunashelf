import type { FormEvent, ReactNode } from "react";
import { useEffect, useMemo, useState } from "react";
import { getStats } from "@kizunashelf/api-contract";
import {
  BarChart3Icon,
  CalendarDaysIcon,
  DatabaseIcon,
  HomeIcon,
  type LucideIcon,
  Link2Icon,
  SearchIcon,
  SettingsIcon,
  TablePropertiesIcon,
} from "lucide-react";
import { Link, NavLink, useLocation, useNavigate } from "react-router-dom";

import { apiFetch, isAbortError } from "@/api/client";
import { ThemeModeSelect } from "@/components/layout/theme-mode-select";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type { StatsResponse } from "@/types/api";

export function AppFrame({ error, children }: { error?: string; children: ReactNode }) {
  const navigate = useNavigate();
  const location = useLocation();
  const [stats, setStats] = useState<StatsResponse>();
  const [search, setSearch] = useState("");
  const activeType = useMemo(() => {
    if (location.pathname !== "/library") return "";
    return new URLSearchParams(location.search).get("type") ?? "";
  }, [location.pathname, location.search]);

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
      location.pathname === "/library" ? new URLSearchParams(location.search) : new URLSearchParams();
    const query = search.trim();
    if (query) params.set("q", query);
    else params.delete("q");
    params.set("page", "1");
    navigate(`/library?${params.toString()}`);
  }

  return (
    <main className="min-h-screen bg-background text-foreground">
      <header className="flex min-h-14 flex-wrap items-center gap-2 border-b bg-card/85 px-3 py-2 sm:flex-nowrap sm:gap-3 sm:px-4">
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
        <form onSubmit={submitSearch} className="order-3 flex w-full min-w-0 items-center gap-2 sm:order-none sm:ml-auto sm:max-w-sm">
          <SearchIcon className="text-muted-foreground" />
          <Input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Search library"
            className="min-w-0"
            aria-label="Search library"
          />
        </form>
        <div className="ml-auto flex items-center gap-2 sm:ml-0">
          <ThemeModeSelect />
          <Badge variant="secondary" className="hidden sm:inline-flex">
            v0.1 MVP
          </Badge>
        </div>
      </header>

      {error ? (
        <div className="border-b bg-destructive/10 px-4 py-2 text-sm text-destructive">
          {error}
        </div>
      ) : null}

      <div className="grid min-h-[calc(100vh-3.5rem)] grid-cols-1 md:grid-cols-[224px_minmax(0,1fr)]">
        <AppSidebar stats={stats} activeType={activeType} pathname={location.pathname} />
        <div className="min-w-0 overflow-hidden">{children}</div>
      </div>
    </main>
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
    <aside className="hidden border-r bg-card/35 md:block">
      <div className="sticky top-0 flex max-h-[calc(100vh-3.5rem)] flex-col gap-4 overflow-auto p-3">
        <section className="flex flex-col gap-1">
          <SidebarSectionLabel>Core Views</SidebarSectionLabel>
          <SidebarNavLink to="/" icon={HomeIcon} end>
            Home
          </SidebarNavLink>
          <SidebarNavLink to="/calendar" icon={CalendarDaysIcon}>
            Calendar
          </SidebarNavLink>
          <SidebarNavLink to="/relations" icon={Link2Icon}>
            Relations
          </SidebarNavLink>
        </section>

        <section className="flex flex-col gap-1">
          <SidebarSectionLabel>Taxonomy</SidebarSectionLabel>
          <SidebarNavLink
            to="/library"
            icon={DatabaseIcon}
            active={pathname === "/library" && !activeType}
          >
            Library
          </SidebarNavLink>
          {stats?.byType.map((type) => (
            <SidebarNavLink
              key={type.id}
              to={`/library?type=${encodeURIComponent(type.id)}`}
              icon={type.icon ? undefined : TablePropertiesIcon}
              emoji={type.icon}
              active={activeType === type.id}
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
          <SidebarNavLink to="/statistics" icon={BarChart3Icon}>
            Statistics
          </SidebarNavLink>
          <button
            type="button"
            className="flex h-8 items-center gap-2 rounded-md px-2 text-left text-sm text-muted-foreground opacity-70"
            disabled
          >
            <SettingsIcon />
            <span className="truncate">Settings</span>
          </button>
        </section>
      </div>
    </aside>
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
  children,
}: {
  to: string;
  icon?: LucideIcon;
  emoji?: string | null;
  end?: boolean;
  active?: boolean;
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
