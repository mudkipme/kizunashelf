import { AssetImage } from "@/components/assets/asset-image";
import type { EntitySummary } from "@/types/api";

export function EntityCover({
  entity,
  size = "sm",
  lightbox = false,
}: {
  entity: EntitySummary;
  size?: "sm" | "lg";
  lightbox?: boolean;
}) {
  const className =
    size === "lg" ? "size-20 rounded-md object-cover" : "size-11 rounded-md object-cover";

  return (
    <AssetImage
      src={entity.image}
      className={className}
      lightbox={lightbox}
      fallback={
        <span
          className={`${className} flex items-center justify-center border bg-muted text-xs font-medium text-muted-foreground`}
        >
          {entity.typeLabel.slice(0, 2)}
        </span>
      }
    />
  );
}
