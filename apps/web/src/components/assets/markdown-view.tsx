import { useLingui } from "@lingui/react/macro";
import { CheckIcon } from "lucide-react";
import { createContext, useContext, useMemo } from "react";
import type { MouseEvent, ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import { Link } from "react-router-dom";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";

import { AssetImage } from "@/components/assets/asset-image";
import { cn } from "@/lib/utils";
import type { Relation } from "@/types/api";

// Every class is newline-free, so `transformWikilinks` can only ever rewrite
// *within* a line. That is load-bearing: a rendered task item is addressed back
// to the Markdown source by line number, so the transform must not move lines.
const wikilinkPattern = /\[\[([^\]|#\n]+)(#[^\]|\n]+)?(?:\|([^\]\n]+))?\]\]/g;

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
export function InlineMarkdown({
  markdown,
  relations,
}: {
  markdown: string;
  relations: Relation[];
}) {
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

/**
 * Makes the `- [ ]` checkboxes in a rendered body live.
 *
 * A task item is addressed back to the Markdown source the way the core expects:
 * its 1-based line number plus that line's source text. Nothing about the item is
 * parsed here — the source line goes to the server verbatim and the core owns
 * both the locator check and the `✅` stamp.
 */
export type TaskToggle = {
  onToggle: (line: number, text: string, done: boolean) => void;
  /// Checkboxes stay visible but inert (read-only vault, or a write in flight).
  disabled?: boolean;
};

/// The task item currently being rendered. `li` publishes its source line (the
/// only node in the tree that carries a position); the synthetic checkbox
/// `input` remark-gfm nests inside it reads that line back out.
const TaskLineContext = createContext<number | null>(null);

/// A live replacement for remark-gfm's disabled checkbox `input`. Inline and
/// baseline-nudged so it sits in the run of text exactly where the `- [ ]` was,
/// and styled like the episode-row checkbox so the two read as one control.
function TaskCheckbox({
  checked,
  tasks,
}: {
  checked: boolean;
  tasks: TaskToggle & { lineText: (line: number) => string | undefined };
}) {
  const { t } = useLingui();
  const line = useContext(TaskLineContext);
  const text = line == null ? undefined : tasks.lineText(line);
  const disabled = tasks.disabled || line == null || text == null;

  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      // The item's own text sits beside the box rather than inside it, so the
      // control needs a name of its own.
      aria-label={t`Task`}
      disabled={disabled}
      onClick={(event) => {
        event.stopPropagation();
        if (line == null || text == null) return;
        tasks.onToggle(line, text, !checked);
      }}
      className={cn(
        "inline-flex size-4 shrink-0 items-center justify-center rounded border align-[-0.2em] transition-colors",
        checked ? "border-primary bg-primary text-primary-foreground" : "border-input",
        disabled ? "cursor-default" : "cursor-pointer hover:border-primary",
      )}
    >
      {checked ? <CheckIcon className="size-3" /> : null}
    </button>
  );
}

export function MarkdownView({
  markdown,
  relations,
  tasks,
}: {
  markdown: string;
  relations: Relation[];
  /// Opt-in: makes `- [ ]` items in this body clickable. Omitted (list
  /// descriptions, the episodes section's prose) they render as plain checkboxes.
  tasks?: TaskToggle;
}) {
  const transformed = transformWikilinks(markdown, relations);
  // The *untransformed* source lines — what the server has on disk. Wikilink
  // rewriting stays within a line, so a node's line number indexes both.
  const sourceLines = useMemo(() => markdown.split("\n"), [markdown]);
  const taskProps = tasks
    ? {
        ...tasks,
        lineText: (line: number) => sourceLines[line - 1]?.replace(/\r$/, ""),
      }
    : undefined;

  return (
    <div className="flex flex-col gap-4 text-prose text-foreground">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkBreaks]}
        components={{
          input({ node, ...props }) {
            const properties = node?.properties;
            if (!taskProps || properties?.type !== "checkbox") {
              return <input {...props} />;
            }
            return <TaskCheckbox checked={properties.checked === true} tasks={taskProps} />;
          },
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
                fallback={<span className="text-xs text-muted-foreground">{text || "image"}</span>}
              />
            );
          },
          h1({ children }) {
            return <h1 className="text-xl leading-8 font-semibold">{children}</h1>;
          },
          h2({ children }) {
            return <h2 className="border-b pb-1 text-lg leading-8 font-semibold">{children}</h2>;
          },
          h3({ children }) {
            return <h3 className="text-base leading-7 font-semibold">{children}</h3>;
          },
          h4({ children }) {
            return <h4 className="text-sm leading-7 font-semibold">{children}</h4>;
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
          li({ node, children }) {
            const className = node?.properties?.className;
            const isTask = Array.isArray(className) && className.includes("task-list-item");
            if (!isTask) return <li className="pl-1">{children}</li>;
            // No bullet beside a checkbox (github-markdown-css does the same).
            // The list indent itself stays, so nesting still reads as nesting.
            return (
              <TaskLineContext.Provider value={node?.position?.start.line ?? null}>
                <li className="list-none pl-1">{children}</li>
              </TaskLineContext.Provider>
            );
          },
          blockquote({ children }) {
            return (
              <blockquote className="border-l-2 pl-4 text-muted-foreground">{children}</blockquote>
            );
          },
          code({ children, className }) {
            const inline = !className;
            if (inline) {
              return <code className="rounded bg-muted px-1 py-0.5 text-xs">{children}</code>;
            }

            return <code className={className}>{children}</code>;
          },
          pre({ children }) {
            return (
              <pre className="overflow-auto rounded-md bg-muted p-3 text-xs leading-5">
                {children}
              </pre>
            );
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

  return markdown.replace(
    wikilinkPattern,
    (_, title: string, heading: string | undefined, alias: string | undefined) => {
      const targetId = targetByTitle.get(title);
      const label = alias || title;

      if (!targetId) return label;

      const anchor = heading ? `#${slugifyObsidianHeading(heading.slice(1))}` : "";
      return `[${escapeMarkdownLabel(label)}](/entities/${encodeURIComponent(targetId)}${anchor})`;
    },
  );
}

function escapeMarkdownLabel(value: string) {
  return value.replace(/([\\[\]])/g, "\\$1");
}

function slugifyObsidianHeading(value: string) {
  return encodeURIComponent(value.trim().toLocaleLowerCase().replace(/\s+/g, "-"));
}
