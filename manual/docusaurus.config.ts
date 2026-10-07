import type * as Preset from "@docusaurus/preset-classic";
import type { Config } from "@docusaurus/types";
import { themes as prismThemes } from "prism-react-renderer";

// The manual is served at the site root: `/introduction/`, `/reference/config/`, …
// (`routeBasePath: "/"`), with the standalone landing page at `/` coming from
// `src/pages/index.tsx`. Keep it that way — those URLs are public.
const config: Config = {
  title: "KizunaShelf Manual",
  tagline: "The KizunaShelf user manual: concepts, guides, and the full schema reference.",
  favicon: "icons/favicon.ico",

  url: "https://kizunashelf.app",
  baseUrl: "/",
  trailingSlash: true,

  onBrokenLinks: "throw",
  onBrokenAnchors: "throw",

  future: {
    v4: true,
    faster: true,
  },

  markdown: {
    // Plain `.md` pages are parsed as CommonMark (so their `<!-- TODO -->` and
    // GENERATED banners stay comments); only `.mdx` pages get JSX, which is what
    // the `<Screenshot>` pages need.
    format: "detect",
    hooks: {
      onBrokenMarkdownLinks: "throw",
    },
  },

  // The manual currently ships in English.
  i18n: {
    defaultLocale: "en",
    locales: ["en"],
  },

  headTags: [
    {
      tagName: "link",
      attributes: {
        rel: "preconnect",
        href: "https://cdn.jsdelivr.net",
        crossorigin: "anonymous",
      },
    },
    {
      tagName: "link",
      attributes: {
        rel: "icon",
        type: "image/png",
        sizes: "96x96",
        href: "/icons/favicon-96x96.png",
      },
    },
    {
      tagName: "link",
      attributes: {
        rel: "apple-touch-icon",
        sizes: "180x180",
        href: "/icons/apple-touch-icon.png",
      },
    },
    {
      tagName: "link",
      attributes: { rel: "manifest", href: "/icons/site.webmanifest" },
    },
  ],

  presets: [
    [
      "classic",
      {
        docs: {
          path: "content",
          routeBasePath: "/",
          sidebarPath: "./sidebars.ts",
          breadcrumbs: true,
        },
        blog: false,
        pages: {},
        theme: {
          customCss: "./src/css/custom.css",
        },
      } satisfies Preset.Options,
    ],
  ],

  themes: [
    [
      // Offline search, replacing Zola's built-in index. Algolia would need an
      // account; this indexes at build time and ships with the site.
      "@easyops-cn/docusaurus-search-local",
      {
        hashed: true,
        indexBlog: false,
        docsDir: "content",
        docsRouteBasePath: "/",
        highlightSearchTermsOnTargetPage: true,
        searchResultLimits: 10,
      },
    ],
  ],

  themeConfig: {
    image: "images/og-image.png",
    colorMode: {
      defaultMode: "light",
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: "KizunaShelf",
      logo: {
        alt: "",
        src: "assets/icon.webp",
        srcDark: "assets/icon-dark.webp",
        href: "/",
        target: "_self",
      },
      items: [
        { to: "/introduction/", label: "Manual", position: "left" },
        { to: "/reference/config/", label: "Reference", position: "left" },
        {
          href: "https://testflight.apple.com/join/hE7k3sWd",
          label: "iOS beta",
          position: "right",
        },
      ],
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.vsDark,
      additionalLanguages: ["bash", "yaml", "toml", "json", "rust", "docker"],
    },
    tableOfContents: {
      minHeadingLevel: 2,
      maxHeadingLevel: 3,
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
