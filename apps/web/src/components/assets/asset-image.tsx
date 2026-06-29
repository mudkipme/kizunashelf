import { useState, type ReactNode } from "react";

import { useLightboxImage } from "@/components/assets/image-lightbox";
import { resolveAssetSrc } from "@/lib/asset-src";
import { cn } from "@/lib/utils";

/**
 * Renders an image from a frontmatter image value, resolving local vault assets
 * to the API asset route and falling back to `fallback` when the value is empty
 * or the image fails to load.
 *
 * With `lightbox`, the rendered image becomes clickable and opens full-screen in
 * the nearest {@link LightboxProvider}; without a provider it stays a plain image.
 */
export function AssetImage({
  src,
  alt = "",
  className,
  fallback,
  lightbox = false,
}: {
  src: string | null | undefined;
  alt?: string;
  className?: string;
  fallback: ReactNode;
  lightbox?: boolean;
}) {
  const resolved = resolveAssetSrc(src);
  const [failedSrc, setFailedSrc] = useState<string>();
  const shown = resolved && failedSrc !== resolved ? resolved : undefined;
  const lb = useLightboxImage(lightbox ? shown : undefined);

  if (!shown) {
    return <>{fallback}</>;
  }

  const interactive = lightbox && lb.enabled;

  return (
    <img
      src={shown}
      alt={alt}
      className={cn(className, interactive && "cursor-zoom-in")}
      loading="lazy"
      decoding="async"
      onError={() => setFailedSrc(shown)}
      {...(interactive
        ? {
            role: "button",
            tabIndex: 0,
            onClick: lb.open,
            onKeyDown: (event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                lb.open();
              }
            },
          }
        : {})}
    />
  );
}
