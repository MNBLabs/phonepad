// @ts-check
import { defineConfig } from "astro/config";
import sitemap from "@astrojs/sitemap";

/**
 * The one place the deployed origin is written down.
 *
 * Canonical URLs, the sitemap and social-card URLs all derive from this, and
 * every internal link on the site is relative. Moving PhonePad to its own
 * domain later is therefore a change to this constant and a DNS record, not a
 * rewrite.
 */
const SITE = process.env.PUBLIC_SITE_URL ?? "https://phonepad.dynshift.com";

export default defineConfig({
  site: SITE,
  trailingSlash: "always",
  build: { format: "directory" },
  integrations: [sitemap()],
  // No client framework, no hydration. The whole site is static HTML with one
  // small inline script; anything more would be cost with no benefit.
});
