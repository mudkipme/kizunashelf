import { Button } from "@/components/ui/button";

export function TypeButton({
  active,
  label,
  count,
  onClick,
}: {
  active: boolean;
  label: string;
  count: number;
  onClick: () => void;
}) {
  return (
    <Button
      variant={active ? "secondary" : "ghost"}
      size="sm"
      className="justify-between"
      onClick={onClick}
    >
      <span className="truncate">{label}</span>
      <span className="tabular-nums text-muted-foreground">{count}</span>
    </Button>
  );
}
