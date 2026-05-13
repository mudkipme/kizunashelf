import type { ReactNode } from "react";
import { BoxesIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { Badge } from "@/components/ui/badge";

export function AppFrame({ error, children }: { error?: string; children: ReactNode }) {
  return (
    <main className="min-h-screen bg-background text-foreground">
      <header className="flex h-12 items-center gap-3 border-b px-4">
        <Link to="/" className="flex items-center gap-3">
          <BoxesIcon className="text-muted-foreground" />
          <span className="min-w-0">
            <span className="block truncate text-sm font-semibold">KizunaShelf</span>
            <span className="block truncate text-xs text-muted-foreground">
              Obsidian Taxonomy read-only asset graph
            </span>
          </span>
        </Link>
        <div className="flex-1" />
        <Badge variant="secondary">v0.1 MVP</Badge>
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
