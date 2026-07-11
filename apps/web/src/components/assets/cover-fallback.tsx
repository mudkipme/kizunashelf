import { useQuery } from "@tanstack/react-query";
import { ImageIcon } from "lucide-react";

import { configQuery } from "@/api/queries";
import { typeIconsById } from "@/lib/type-config";
import { cn } from "@/lib/utils";

/**
 * The unified cover placeholder, used as the `fallback` for every entity cover
 * so a type without a cover field, an empty value, and a broken image all look
 * the same everywhere. Shows the entity type's schema emoji when the type
 * declares one, otherwise a generic image glyph — mirroring iOS's type-icon
 * cover placeholder (`EntityCoverThumbnail`).
 *
 * It resolves the icon from the (cached) config query itself, so callers pass
 * only the entity's `type` id. `className` tunes the slot (size / shape); it
 * defaults to filling its container.
 */
export function CoverFallback({
  type,
  className,
}: {
  type: string | null | undefined;
  className?: string;
}) {
  const config = useQuery(configQuery());
  const icon = type ? typeIconsById(config.data?.types).get(type) : undefined;

  return (
    <span
      className={cn(
        "flex items-center justify-center bg-muted text-muted-foreground",
        // Fill the slot by default; a caller that owns the sizing (fixed square,
        // aspect ratio) passes its own so `size-full` doesn't fight it.
        className || "size-full",
      )}
      aria-hidden="true"
    >
      {icon ? (
        <span className="text-2xl leading-none">{icon}</span>
      ) : (
        <ImageIcon className="size-5" />
      )}
    </span>
  );
}
