import { useEffect, useState } from "react";
import { describe, expect, it } from "vitest";
import { page } from "vitest/browser";

import { AssetImage } from "@/components/assets/asset-image";
import { LightboxProvider } from "@/components/assets/image-lightbox";
import { render } from "@/test/render";

/// Distinct, genuinely loadable sources: a src that 404s would trip
/// `AssetImage`'s `onError` and unregister the image out from under the test.
const src = (size: number) =>
  `data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='${size}' height='${size}'%3E%3C/svg%3E`;

const ADD_SECOND = "lightbox-test:add-second";

/// A region whose second image mounts after the first, which is what a detail
/// page does when a note's images arrive behind the cover. The trigger is a
/// window event rather than a button because the open overlay covers the page.
function Region() {
  const [second, setSecond] = useState(false);
  useEffect(() => {
    const add = () => setSecond(true);
    window.addEventListener(ADD_SECOND, add);
    return () => window.removeEventListener(ADD_SECOND, add);
  }, []);

  return (
    <LightboxProvider>
      <AssetImage src={src(2)} alt="first" fallback={null} lightbox />
      {second ? <AssetImage src={src(4)} alt="second" fallback={null} lightbox /> : null}
    </LightboxProvider>
  );
}

describe("LightboxProvider", () => {
  it("opens the gallery on the clicked image and keeps it open", async () => {
    const screen = await render(<Region />);

    await screen.getByRole("button", { name: "first" }).click();

    // The provider re-renders on open; if that re-ran the descendants'
    // registration effects, their cleanup would unregister and close the
    // overlay in the same breath.
    await expect.element(page.getByRole("button", { name: "Close" })).toBeVisible();
  });

  it("picks up an image that mounts while the gallery is already open", async () => {
    const screen = await render(<Region />);

    await screen.getByRole("button", { name: "first" }).click();
    await expect.element(page.getByRole("button", { name: "Close" })).toBeVisible();
    // One slide: the app hides the navigation chrome rather than show arrows
    // that go nowhere, so its absence stands in for the slide count.
    expect(page.getByRole("button", { name: "Next" }).elements()).toHaveLength(0);

    window.dispatchEvent(new Event(ADD_SECOND));

    await expect.element(page.getByRole("button", { name: "Next" })).toBeVisible();
  });
});
