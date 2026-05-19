import type { ReactNode } from "react";
import { Link, NavLink } from "react-router-dom";

import { ThemeModeSelect } from "@/components/layout/theme-mode-select";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

export function AppFrame({ error, children }: { error?: string; children: ReactNode }) {
  return (
    <main className="min-h-screen bg-background text-foreground">
      <header className="flex min-h-12 flex-wrap items-center gap-2 border-b bg-card/85 px-3 py-2 sm:flex-nowrap sm:gap-3 sm:px-4">
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
        <nav className="order-3 flex w-full min-w-0 items-center gap-1 overflow-x-auto sm:order-none sm:w-auto">
          <NavLink
            to="/"
            className={({ isActive }) =>
              cn(
                "rounded-md px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                isActive && "bg-accent text-foreground",
              )
            }
          >
            Home
          </NavLink>
          <NavLink
            to="/library"
            className={({ isActive }) =>
              cn(
                "rounded-md px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                isActive && "bg-accent text-foreground",
              )
            }
          >
            Library
          </NavLink>
          <NavLink
            to="/calendar"
            className={({ isActive }) =>
              cn(
                "rounded-md px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                isActive && "bg-accent text-foreground",
              )
            }
          >
            Calendar
          </NavLink>
          <NavLink
            to="/statistics"
            className={({ isActive }) =>
              cn(
                "rounded-md px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                isActive && "bg-accent text-foreground",
              )
            }
          >
            Statistics
          </NavLink>
          <NavLink
            to="/relations"
            className={({ isActive }) =>
              cn(
                "rounded-md px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                isActive && "bg-accent text-foreground",
              )
            }
          >
            Relations
          </NavLink>
        </nav>
        <div className="hidden flex-1 sm:block" />
        <div className="ml-auto flex items-center gap-2 sm:ml-0">
          <ThemeModeSelect />
          <Badge variant="secondary">v0.1 MVP</Badge>
        </div>
      </header>

      {error ? (
        <div className="border-b bg-destructive/10 px-4 py-2 text-sm text-destructive">
          {error}
        </div>
      ) : null}

      {children}
    </main>
  );
}
