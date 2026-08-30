import type { ReactNode } from "react";

import styles from "./Screenshot.module.css";

type Platforms = "both" | "desktop" | "ios";

type Props = {
  /** Caption rendered under the frames. */
  caption?: string;
  /** Path to the real desktop capture; a dashed placeholder stands in until one exists. */
  desktop?: string;
  /** Path to the real iOS capture; a dashed placeholder stands in until one exists. */
  ios?: string;
  /** Which frames to render. Passing only `desktop` or only `ios` also narrows it. */
  platforms?: Platforms;
};

/**
 * A desktop + iOS screenshot pair, with placeholder boxes until real captures
 * exist. Registered globally in `src/theme/MDXComponents.tsx`, so `.mdx` pages
 * use `<Screenshot caption="…" />` without importing it.
 */
export default function Screenshot({
  caption,
  desktop,
  ios,
  platforms = "both",
}: Props): ReactNode {
  return (
    <figure className={styles.figure}>
      {platforms !== "ios" && (
        <div className={styles.desktopSlot}>
          {desktop ? (
            <img
              className={styles.desktopImage}
              src={desktop}
              alt={caption ? `${caption} (desktop)` : "Desktop screenshot"}
            />
          ) : (
            <div className={styles.desktopPlaceholder}>Desktop screenshot — TODO</div>
          )}
        </div>
      )}
      {platforms !== "desktop" && (
        <div className={styles.iosSlot}>
          {ios ? (
            <img
              className={styles.iosImage}
              src={ios}
              alt={caption ? `${caption} (iOS)` : "iOS screenshot"}
            />
          ) : (
            <div className={styles.iosPlaceholder}>iOS screenshot — TODO</div>
          )}
        </div>
      )}
      {caption && <figcaption className={styles.caption}>{caption}</figcaption>}
    </figure>
  );
}
