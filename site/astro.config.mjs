// @ts-check
import { defineConfig } from "astro/config";
import sitemap from "@astrojs/sitemap";
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";

/**
 * The one place the deployed origin is written down.
 *
 * Canonical URLs, the sitemap and social-card URLs all derive from this, and
 * every internal link on the site is relative. Moving PhonePad to its own
 * domain later is therefore a change to this constant and a DNS record, not a
 * rewrite.
 */
const SITE = process.env.PUBLIC_SITE_URL ?? "https://phonepad.dynshift.com";

/**
 * Each sitemap entry's <lastmod> is when its page source last changed in git.
 * Search engines only trust lastmod that is accurate, so it is never the build
 * date (which would claim every page changed on every deploy). The site
 * workflow checks out full history for this; with no history, or a page not
 * found, the entry simply has no lastmod.
 */
function withLastmod(item) {
  const path = new URL(item.url).pathname.replace(/^\/|\/$/g, "");
  const file = `src/pages/${path || "index"}.astro`;
  if (!existsSync(file)) return item;
  try {
    const date = execFileSync("git", ["log", "-1", "--format=%cI", "--", file], {
      encoding: "utf8",
    }).trim();
    if (date) item.lastmod = date;
  } catch {
    // Not a git checkout: leave lastmod out rather than guess.
  }
  return item;
}

export default defineConfig({
  site: SITE,
  trailingSlash: "always",
  build: { format: "directory" },
  // Astro 7 defaults to JSX-style whitespace, which drops the space between a
  // line of text and a link that starts the next line. Keep Astro 5's rule.
  compressHTML: true,
  integrations: [sitemap({ serialize: withLastmod })],
  // No client framework, no hydration. The whole site is static HTML with one
  // small inline script; anything more would be cost with no benefit.
});
