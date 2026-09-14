import type { ReactNode } from "react";

import styles from "./Screenshot.module.css";

type Props = {
  caption: string;
  desktop?: string;
  desktopAlt?: string;
  ios?: string;
  iosAlt?: string;
  /** Defaults to iOS, or to the platform of a supplied image. */
  platforms?: "both" | "desktop" | "ios";
};

/** Shared frames for manual captures and the homepage's iOS preview spaces. */
export default function Screenshot({
  caption,
  desktop,
  desktopAlt,
  ios,
  iosAlt,
  platforms = desktop ? (ios ? "both" : "desktop") : "ios",
}: Props): ReactNode {
  return (
    <figure className={styles.figure}>
      {platforms !== "ios" && (
        <div className={styles.desktopSlot}>
          {desktop ? (
            <a
              href={desktop}
              target="_blank"
              rel="noreferrer"
              aria-label={`Open full-size desktop screenshot: ${desktopAlt ?? caption}`}
            >
              <img
                className={styles.desktopImage}
                src={desktop}
                alt={desktopAlt ?? caption}
                width={2704}
                height={1786}
                loading="lazy"
                decoding="async"
              />
            </a>
          ) : (
            <div className={styles.desktopPlaceholder}>Desktop preview coming soon</div>
          )}
        </div>
      )}
      {platforms !== "desktop" && (
        <div className={styles.iosSlot}>
          {ios ? (
            <a
              href={ios}
              target="_blank"
              rel="noreferrer"
              aria-label={`Open full-size iOS screenshot: ${iosAlt ?? caption}`}
            >
              <img
                className={styles.iosImage}
                src={ios}
                alt={iosAlt ?? caption}
                loading="lazy"
                decoding="async"
              />
            </a>
          ) : (
            <div className={styles.iosPlaceholder}>iOS preview coming soon</div>
          )}
        </div>
      )}
      <figcaption className={styles.caption}>{caption}</figcaption>
    </figure>
  );
}
