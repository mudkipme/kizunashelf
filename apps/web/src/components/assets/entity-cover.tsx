import { AssetImage } from "@/components/assets/asset-image";
import { CoverFallback } from "@/components/assets/cover-fallback";
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
  const size2 = size === "lg" ? "size-20" : "size-11";

  return (
    <AssetImage
      src={entity.image}
      className={`${size2} rounded-md object-cover`}
      lightbox={lightbox}
      fallback={<CoverFallback type={entity.type} className={`${size2} rounded-md border`} />}
    />
  );
}
