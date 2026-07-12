import { cn } from "@/lib/utils";

const WIDTHS = {
  standard: "max-w-5xl",
  wide: "max-w-7xl",
  full: "",
} as const;

export type PageWidth = keyof typeof WIDTHS;

export function PageContainer({
  width = "standard",
  className,
  children,
}: {
  width?: PageWidth;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <div className={cn("mx-auto flex w-full flex-col gap-4 p-4", WIDTHS[width], className)}>
      {children}
    </div>
  );
}
