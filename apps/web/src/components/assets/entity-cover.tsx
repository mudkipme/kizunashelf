import type { EntitySummary } from "@/types/api";

export function EntityCover({ entity, size = "sm" }: { entity: EntitySummary; size?: "sm" | "lg" }) {
  const className =
    size === "lg" ? "size-20 rounded-md object-cover" : "size-11 rounded-md object-cover";

  if (entity.image) {
    return <img src={entity.image} alt="" className={className} loading="lazy" />;
  }

  return (
    <span
      className={`${className} flex items-center justify-center border bg-muted text-xs font-medium text-muted-foreground`}
    >
      {entity.typeLabel.slice(0, 2)}
    </span>
  );
}
