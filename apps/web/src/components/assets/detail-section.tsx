import type { ReactNode } from "react";

export function DetailSection({
  title,
  icon,
  action,
  children,
}: {
  title: string;
  icon: ReactNode;
  /// Optional control shown on the right of the title row (e.g. a section action).
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="mb-5 flex flex-col gap-2 last:mb-0">
      <div className="flex min-h-7 items-center justify-between gap-2">
        <h3 className="flex items-center gap-2 text-xs font-medium uppercase text-muted-foreground">
          {icon}
          {title}
        </h3>
        {action}
      </div>
      {children}
    </section>
  );
}

export function EmptyLine({ children }: { children: ReactNode }) {
  return <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">{children}</div>;
}
