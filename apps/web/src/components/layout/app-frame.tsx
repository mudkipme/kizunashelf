import type { ReactNode } from "react";
import { BoxesIcon } from "lucide-react";
import { Link, NavLink } from "react-router-dom";

import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

export function AppFrame({ error, children }: { error?: string; children: ReactNode }) {
  return (
    <main className="min-h-screen bg-background text-foreground">
      <header className="flex min-h-12 flex-wrap items-center gap-2 border-b px-3 py-2 sm:flex-nowrap sm:gap-3 sm:px-4">
        <Link to="/" className="flex min-w-0 flex-1 items-center gap-3 sm:flex-none">
          <BoxesIcon className="shrink-0 text-muted-foreground" />
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
        <Badge variant="secondary" className="ml-auto sm:ml-0">
          v0.1 MVP
        </Badge>
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
