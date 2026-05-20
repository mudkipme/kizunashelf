import { readFile, stat } from "node:fs/promises";
import { extname, join, normalize, relative } from "node:path";

export async function staticResponse(
  webDistPath: string,
  pathname: string,
): Promise<Response | undefined> {
  const requestedPath = pathname === "/" ? "/index.html" : decodeURIComponent(pathname);
  const filePath = resolveStaticPath(webDistPath, requestedPath);
  const file = filePath ? await readStaticFile(filePath) : undefined;

  if (file) return file;

  const fallback = await readStaticFile(join(webDistPath, "index.html"));
  return fallback;
}

function resolveStaticPath(webDistPath: string, pathname: string): string | undefined {
  const normalized = normalize(pathname).replace(/^(\.\.(\/|\\|$))+/, "");
  const filePath = join(webDistPath, normalized);
  const relativePath = relative(webDistPath, filePath);

  if (relativePath.startsWith("..") || relativePath === "") return undefined;
  return filePath;
}

async function readStaticFile(filePath: string): Promise<Response | undefined> {
  try {
    const info = await stat(filePath);
    if (!info.isFile()) return undefined;
    const body = await readFile(filePath);
    return new Response(body, {
      headers: {
        "content-type": contentType(filePath),
      },
    });
  } catch {
    return undefined;
  }
}

function contentType(filePath: string) {
  const extension = extname(filePath);
  if (extension === ".html") return "text/html; charset=utf-8";
  if (extension === ".js") return "text/javascript; charset=utf-8";
  if (extension === ".css") return "text/css; charset=utf-8";
  if (extension === ".json") return "application/json; charset=utf-8";
  if (extension === ".svg") return "image/svg+xml";
  if (extension === ".png") return "image/png";
  if (extension === ".jpg" || extension === ".jpeg") return "image/jpeg";
  if (extension === ".webp") return "image/webp";
  if (extension === ".ico") return "image/x-icon";
  return "application/octet-stream";
}
