import { defineConfig } from "orval";

export default defineConfig({
  kizunashelf: {
    input: {
      target: "openapi/kizunashelf.openapi.json",
    },
    output: {
      mode: "single",
      target: "src/generated/client.ts",
      client: "fetch",
      clean: ["src/generated"],
      indexFiles: false,
      schemas: {
        type: "zod",
        path: "src/generated",
      },
      urlEncodeParameters: true,
      override: {
        fetch: {
          includeHttpResponseReturnType: false,
          runtimeValidation: true,
          useRuntimeFetcher: true,
        },
      },
    },
  },
});
