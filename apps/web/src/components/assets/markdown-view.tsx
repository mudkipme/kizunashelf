import ReactMarkdown from "react-markdown";
import { Link } from "react-router-dom";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

import type { Relation } from "@/types/api";

const wikilinkPattern = /\[\[([^\]|#]+)(#[^\]|]+)?(?:\|([^\]]+))?\]\]/g;

export function MarkdownView({ markdown, relations }: { markdown: string; relations: Relation[] }) {
  const transformed = transformWikilinks(markdown, relations);

  return (
    <div className="flex flex-col gap-4 text-sm leading-7 text-foreground">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkBreaks]}
        components={{
          a({ href, children }) {
            if (href?.startsWith("/entities/")) {
              return (
                <Link
                  to={href}
                  className="font-medium text-primary underline-offset-4 hover:underline"
                >
                  {children}
                </Link>
              );
            }

            return (
              <a
                href={href}
                target="_blank"
                rel="noreferrer"
                className="font-medium text-primary underline-offset-4 hover:underline"
              >
                {children}
              </a>
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
