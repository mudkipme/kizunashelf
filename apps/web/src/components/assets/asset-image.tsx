import { useState, type ReactNode } from "react";

import { resolveAssetSrc } from "@/lib/asset-src";

/**
 * Renders an image from a frontmatter image value, resolving local vault assets
 * to the API asset route and falling back to `fallback` when the value is empty
 * or the image fails to load.
 */
export function AssetImage({
  src,
  alt = "",
  className,
  fallback,
}: {
  src: string | null | undefined;
  alt?: string;
  className?: string;
  fallback: ReactNode;
}) {
  const resolved = resolveAssetSrc(src);
  const [failedSrc, setFailedSrc] = useState<string>();

  if (!resolved || failedSrc === resolved) {
    return <>{fallback}</>;
  }

  return (
    <img
      src={resolved}
      alt={alt}
      className={className}
      loading="lazy"
      decoding="async"
      onError={() => setFailedSrc(resolved)}
    />
  );
}
