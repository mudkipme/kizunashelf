import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import Lightbox from "yet-another-react-lightbox";
import Zoom from "yet-another-react-lightbox/plugins/zoom";

import "yet-another-react-lightbox/styles.css";

type Registration = { id: string; src: string };

type LightboxApi = {
  /** Register a visible image and get a stable id back. */
  register: (src: string) => string;
  /** Stop tracking an image (on unmount). */
  unregister: (id: string) => void;
  /** Open the gallery focused on the image with this id. */
  open: (id: string) => void;
};

const LightboxContext = createContext<LightboxApi | null>(null);

/**
 * Wraps a region whose images can be opened full-screen. Every descendant
 * `AssetImage` with `lightbox` enabled registers itself here in mount order, so
 * the overlay shows the whole region's images as one gallery and starts on the
 * one the user tapped. Renders a single shared {@link Lightbox} for the region.
 */
export function LightboxProvider({ children }: { children: ReactNode }) {
  // The registry order mirrors mount order, which matches document order for the
  // detail page (cover first, then the notes images top-to-bottom). It is state
  // rather than a ref because the slide list is render output: an image that
  // mounts while the gallery is already open still has to reach the overlay.
  // Registering re-renders this component alone — `children` arrives as an
  // unchanged element and `api` is stable, so neither the subtree below nor its
  // context consumers re-render along with it.
  const [entries, setEntries] = useState<Registration[]>([]);
  const nextId = useRef(0);
  const [openId, setOpenId] = useState<string | null>(null);

  const register = useCallback((src: string) => {
    // Minted outside the updater so it can be returned synchronously, which
    // keeps the updater itself pure and safe to re-run.
    const id = `lb-${nextId.current++}`;
    setEntries((current) => [...current, { id, src }]);
    return id;
  }, []);

  const unregister = useCallback((id: string) => {
    setEntries((current) => current.filter((entry) => entry.id !== id));
    setOpenId((current) => (current === id ? null : current));
  }, []);

  const open = useCallback((id: string) => setOpenId(id), []);

  // Stable identity: the callbacks never change, so descendant `useLightboxImage`
  // effects register once. A fresh object here would change `api` on every render
  // (e.g. when `openId` updates), re-running those effects — their cleanup would
  // unregister and reset `openId`, closing the lightbox the instant it opened.
  const api = useMemo<LightboxApi>(
    () => ({ register, unregister, open }),
    [register, unregister, open],
  );

  // Memoised so an `openId` change hands the open overlay the same slide array
  // it already has, rather than a fresh one mid-gallery.
  const slides = useMemo(() => entries.map((entry) => ({ src: entry.src })), [entries]);
  const index = openId ? entries.findIndex((entry) => entry.id === openId) : -1;
  const isOpen = index >= 0;

  return (
    <LightboxContext.Provider value={api}>
      {children}
      <Lightbox
        open={isOpen}
        close={() => setOpenId(null)}
        index={index < 0 ? 0 : index}
        slides={slides}
        plugins={[Zoom]}
        controller={{ closeOnBackdropClick: true }}
        carousel={{ finite: true }}
        // Hide the prev/next chrome when there's nothing to navigate to.
        render={
          slides.length <= 1
            ? { buttonPrev: () => null, buttonNext: () => null }
            : undefined
        }
      />
    </LightboxContext.Provider>
  );
}

/**
 * Registers `src` with the nearest {@link LightboxProvider} and returns a click
 * handler that opens the gallery on it. When there is no provider (or no src),
 * `enabled` is false and the image stays a plain, non-interactive image.
 */
export function useLightboxImage(src: string | null | undefined) {
  const api = useContext(LightboxContext);
  const idRef = useRef<string | null>(null);

  useEffect(() => {
    if (!api || !src) {
      idRef.current = null;
      return;
    }
    const id = api.register(src);
    idRef.current = id;
    return () => api.unregister(id);
  }, [api, src]);

  const open = useCallback(() => {
    if (api && idRef.current) api.open(idRef.current);
  }, [api]);

  return { enabled: Boolean(api && src), open };
}
