import MDXComponents from "@theme-original/MDXComponents";

import Screenshot from "@site/src/components/Screenshot";

// Registered globally so `.mdx` pages can use <Screenshot /> without an import,
// the way they used Zola's `{{ screenshot(...) }}` shortcode.
export default {
  ...MDXComponents,
  Screenshot,
};
