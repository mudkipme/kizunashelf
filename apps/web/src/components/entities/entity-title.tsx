import type { ElementType } from "react";

import { entityTitleParts } from "@/lib/title-language";
import type { EntitySummary } from "@/types/api";

/**
 * An entity's display title, stamping `lang` only when the resolved title is in
 * a different language than the viewer's — so the browser picks the correct Han
 * glyph variant for a title shown in a foreign UI (a Japanese title in a Chinese
 * UI, etc.). When the title is in the viewer's own language it inherits the
 * document `lang` (which carries the script, e.g. `zh-Hant`), so nothing is
 * stamped — avoids downgrading a scripted document locale to a bare `zh`.
 */
export function EntityTitle({
  entity,
  language,
  as: Tag = "span",
  className,
}: {
  entity: Pick<EntitySummary, "title" | "titles">;
  language: string;
  as?: ElementType;
  className?: string;
}) {
  const { text, lang } = entityTitleParts(entity, language);
  const stamp = lang && lang !== language ? lang : undefined;
  return (
    <Tag className={className} lang={stamp}>
      {text}
    </Tag>
  );
}
