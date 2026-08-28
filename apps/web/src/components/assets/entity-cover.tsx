import { AssetImage } from "@/components/assets/asset-image";
import { CoverFallback } from "@/components/assets/cover-fallback";
import type { EntitySummary } from "@/types/api";

/**
 * An entity's cover as a fixed square thumbnail, for lists and pickers where a
 * predictable row height matters more than the image's own proportions.
 *
 * The detail page deliberately doesn't use this — it has one image and the room
 * to show it at its real aspect ratio (see `HeroCover`).
 */
export function EntityCover({ entity }: { entity: EntitySummary }) {
  return (
    <AssetImage
      src={entity.image}
      className="size-11 rounded-md object-cover"
      fallback={
        <CoverFallback
          type={entity.type}
          className="size-11 rounded-md ring-1 ring-black/5 dark:ring-white/10"
        />
      }
    />
  );
}
