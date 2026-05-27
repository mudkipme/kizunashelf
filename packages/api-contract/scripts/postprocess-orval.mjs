import { readFile, writeFile } from "node:fs/promises";

const generatedPath = new URL("../src/generated.ts", import.meta.url);
let source = await readFile(generatedPath, "utf8");

source = source.replace(
  'import { makeApi, Zodios, type ZodiosOptions } from "@zodios/core";\n',
  "",
);

source = source.replaceAll("z.record(z.string())", "z.record(z.string(), z.string())");

source = source.replace(/^type ([A-Za-z0-9_]+) =/gm, "export type $1 =");

const endpointsStart = source.indexOf("\nconst endpoints = makeApi([");
if (endpointsStart !== -1) {
  source = source.slice(0, endpointsStart).trimEnd() + "\n";
}

await writeFile(generatedPath, source);
