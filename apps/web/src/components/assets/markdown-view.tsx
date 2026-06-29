import type { MouseEvent, ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import { Link } from "react-router-dom";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

import { AssetImage } from "@/components/assets/asset-image";
import type { Relation } from "@/types/api";

const wikilinkPattern = /\[\[([^\]|#]+)(#[^\]|]+)?(?:\|([^\]]+))?\]\]/g;

const linkClassName = "font-medium text-primary underline-offset-4 hover:underline";

// Renders a resolved link (internal entity links via the router, external links
// in a new tab). Stops click propagation so the link still works when it sits
// inside a clickable parent (e.g. a toggleable episode row).
function MarkdownLink({ href, children }: { href?: string; children?: ReactNode }) {
  const stop = (event: MouseEvent) => event.stopPropagation();

  if (href?.startsWith("/entities/")) {
    return (
      <Link to={href} className={linkClassName} onClick={stop}>
        {children}
      </Link>
    );
  }

  return (
    <a href={href} target="_blank" rel="noreferrer" className={linkClassName} onClick={stop}>
      {children}
    </a>
  );
}

/**
 * Renders a short, single-line string (e.g. an episode title) with wikilink and
 * basic inline Markdown support. Unlike {@link MarkdownView} it produces inline
 * content — the paragraph wrapper is collapsed — so it can sit inside a list row.
 */
export function InlineMarkdown({ markdown, relations }: { markdown: string; relations: Relation[] }) {
  const transformed = transformWikilinks(markdown, relations);

  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      components={{
        p({ children }) {
          return <>{children}</>;
        },
        a({ href, children }) {
          return <MarkdownLink href={href}>{children}</MarkdownLink>;
        },
      }}
    >
      {transformed}
    </ReactMarkdown>
  );
}

export function MarkdownView({ markdown, relations }: { markdown: string; relations: Relation[] }) {
  const transformed = transformWikilinks(markdown, relations);

  return (
    <div className="flex flex-col gap-4 text-sm leading-7 text-foreground">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkBreaks]}
        components={{
          a({ href, children }) {
            return <MarkdownLink href={href}>{children}</MarkdownLink>;
          },
          img({ src, alt }) {
            if (typeof src !== "string" || !src.trim()) return null;
            const text = typeof alt === "string" ? alt : "";
            return (
              <AssetImage
                src={src}
                alt={text}
                lightbox
                className="my-1 max-h-96 max-w-full rounded-md border object-contain"
                fallback={
                  <span className="text-xs text-muted-foreground">{text || "image"}</span>
                }
              />
            );
          },
          h1({ children }) {
            return <h1 className="text-xl font-semibold leading-8">{children}</h1>;
          },
          h2({ children }) {
            return <h2 className="border-b pb-1 text-lg font-semibold leading-8">{children}</h2>;
          },
          h3({ children }) {
            return <h3 className="text-base font-semibold leading-7">{children}</h3>;
          },
          h4({ children }) {
            return <h4 className="text-sm font-semibold leading-7">{children}</h4>;
          },
          p({ children }) {
            return <p>{children}</p>;
          },
          ul({ children }) {
            return <ul className="ml-5 list-disc">{children}</ul>;
          },
          ol({ children }) {
            return <ol className="ml-5 list-decimal">{children}</ol>;
          },
          li({ children }) {
            return <li className="pl-1">{children}</li>;
          },
          blockquote({ children }) {
            return <blockquote className="border-l-2 pl-4 text-muted-foreground">{children}</blockquote>;
          },
          code({ children, className }) {
            const inline = !className;
            if (inline) {
              return <code className="rounded bg-muted px-1 py-0.5 text-xs">{children}</code>;
            }

            return <code className={className}>{children}</code>;
          },
          pre({ children }) {
            return <pre className="overflow-auto rounded-md bg-muted p-3 text-xs leading-5">{children}</pre>;
          },
          table({ children }) {
            return (
              <div className="overflow-auto">
                <table className="w-full border-collapse text-sm">{children}</table>
              </div>
            );
          },
          th({ children }) {
            return <th className="border px-2 py-1 text-left font-medium">{children}</th>;
          },
          td({ children }) {
            return <td className="border px-2 py-1 align-top">{children}</td>;
          },
        }}
      >
        {transformed}
      </ReactMarkdown>
    </div>
  );
}

function transformWikilinks(markdown: string, relations: Relation[]) {
  const targetByTitle = new Map<string, string>();
  for (const relation of relations) {
    if (relation.targetId) targetByTitle.set(relation.targetTitle, relation.targetId);
  }

  return markdown.replace(wikilinkPattern, (_, title: string, heading: string | undefined, alias: string | undefined) => {
    const targetId = targetByTitle.get(title);
    const label = alias || title;

    if (!targetId) return label;

    const anchor = heading ? `#${slugifyObsidianHeading(heading.slice(1))}` : "";
    return `[${escapeMarkdownLabel(label)}](/entities/${encodeURIComponent(targetId)}${anchor})`;
  });
}

function escapeMarkdownLabel(value: string) {
  return value.replace(/([\\[\]])/g, "\\$1");
}

function slugifyObsidianHeading(value: string) {
  return encodeURIComponent(value.trim().toLocaleLowerCase().replace(/\s+/g, "-"));
}
