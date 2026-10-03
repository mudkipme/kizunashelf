import { useEffect, useRef } from "react";

import type { FrontmatterDraft } from "@/components/entities/metadata-types";
import { uploadImageFile } from "@/lib/image-upload";

/** Device images stay local until Create. Retries reuse successful uploads. */
export function useCreateImages() {
  const files = useRef(new Map<string, File>());
  const uploaded = useRef(new Map<string, string>());

  useEffect(() => {
    const previews = files.current;
    return () => {
      for (const url of previews.keys()) URL.revokeObjectURL(url);
      previews.clear();
    };
  }, []);

  async function stageImage(_field: string, file: File) {
    const url = URL.createObjectURL(file);
    files.current.set(url, file);
    return url;
  }

  function hasPending(frontmatter: FrontmatterDraft) {
    return Object.values(frontmatter).some((value) =>
      (Array.isArray(value) ? value : [value]).some(
        (item) => typeof item === "string" && files.current.has(item),
      ),
    );
  }

  function withoutPending(frontmatter: FrontmatterDraft): FrontmatterDraft {
    return Object.fromEntries(
      Object.entries(frontmatter).map(([field, value]) => [
        field,
        Array.isArray(value)
          ? value.filter((item) => typeof item !== "string" || !files.current.has(item))
          : typeof value === "string" && files.current.has(value)
            ? null
            : value,
      ]),
    );
  }

  async function uploadImages(entityId: string, frontmatter: FrontmatterDraft) {
    const next = { ...frontmatter };
    for (const [field, value] of Object.entries(frontmatter)) {
      const resolved = [];
      for (const item of Array.isArray(value) ? value : [value]) {
        const file = typeof item === "string" ? files.current.get(item) : undefined;
        if (!file) {
          resolved.push(item);
          continue;
        }
        const key = `${field}\0${item}`;
        let path = uploaded.current.get(key);
        if (!path) {
          path = await uploadImageFile(entityId, field, file);
          uploaded.current.set(key, path);
        }
        resolved.push(path);
      }
      next[field] = Array.isArray(value) ? resolved : resolved[0];
    }
    return next;
  }

  return { stageImage, hasPending, withoutPending, uploadImages };
}
