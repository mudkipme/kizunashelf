import type { ReactNode } from "react";

export function AnalyticsSection({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children: ReactNode;
}) {
  return (
    <section className="min-w-0 rounded-md border">
      <header className="border-b px-3 py-2">
        <h2 className="text-sm font-semibold">{title}</h2>
        {subtitle ? <p className="mt-1 text-xs text-muted-foreground">{subtitle}</p> : null}
      </header>
      <div className="p-3">{children}</div>
    </section>
  );
}
