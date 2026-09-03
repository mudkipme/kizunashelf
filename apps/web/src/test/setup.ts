// Loads the app's real stylesheet into every UI test page.
//
// This is not cosmetic. Assertions like `toBeVisible()` read computed styles, so
// without Tailwind a `hidden` utility class hides nothing and a collapsed panel
// would assert as visible. Loading the same CSS the app ships keeps those checks
// honest.
import "@/index.css";
// Importing this activates the bundled `en` catalog as a side effect, so the
// Lingui macros in components resolve to real English strings rather than
// message ids. Tests therefore assert on the same text a user reads.
import "@/lib/i18n";
