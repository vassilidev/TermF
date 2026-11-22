---
name: seo
description: >-
  Makes pages indexable and well presented in Google Search with the framework's own metadata API: titles and
  descriptions, canonical URLs, hreflang, sitemaps and robots rules, structured data that is still eligible
  for rich results, Open Graph cards, JavaScript rendering pitfalls, Core Web Vitals, and what Google says
  about AI Overviews and AI Mode. Use when a task mentions SEO, search ranking, indexing, meta tags, sitemap,
  robots.txt, canonical, hreflang, schema.org or JSON-LD, social previews, or Lighthouse SEO scores. Not for
  accessibility (accessibility), styling (css-tailwind) or paid ads.
license: MIT
metadata:
  termf-stacks: "web"
  termf-roles: "seo front"
  termf-versions: "*"
  termf-verified: "2026-11-22 Google Search Central docs and updates log"
  termf-sources: "https://developers.google.com/search/updates https://developers.google.com/search/docs/fundamentals/ai-optimization-guide https://developers.google.com/search/docs/crawling-indexing/robots-meta-tag https://developers.google.com/search/docs/crawling-indexing/control-what-you-share https://developers.google.com/search/docs/crawling-indexing/javascript/javascript-seo-basics https://developers.google.com/search/docs/appearance/structured-data/faqpage https://developers.google.com/search/docs/appearance/structured-data/search-gallery https://developers.google.com/crawling/docs/crawlers-fetchers/google-common-crawlers https://web.dev/articles/vitals"
---
# SEO

No search engine publishes an official agent skill; Google Search Central is the reference, and it changes often: check its updates log (https://developers.google.com/search/updates) for anything newer than `termf-verified`. The community `AgriciDaniel/claude-seo` (MIT) is broader and unaudited.

1. Find the framework and its metadata API, and use it instead of hand-written tags: Next.js `metadata` / `generateMetadata` plus `app/sitemap.ts` and `app/robots.ts`; Nuxt `useSeoMeta` and `useHead`; SvelteKit `<svelte:head>` fed by `load`; Angular `Title` and `Meta` services; Astro or plain HTML: the shared layout's `<head>`. Done when you know the one place each page's head comes from.
2. Measure before changing: fetch the page from the local dev server (in a TermF worktree its port is in `PORT`) and read the initial HTML with `curl -s http://localhost:$PORT/<path>`, then run Lighthouse on it. Done when you have the current title, description, canonical, robots, JSON-LD and scores written down.
3. Make the initial HTML of each indexable page carry a unique `<title>`, a meta description, an absolute self-referencing canonical, and no `noindex`; build absolute URLs from the configured site URL, never from the request's `Host` header. Done when `curl` shows them without running JavaScript.
4. For several languages or regions, give every version `hreflang` links to all versions including itself, plus `x-default`, with absolute URLs that match the canonicals. Done when each pair of pages links to each other.
5. List only canonical, indexable URLs that answer 200 in the sitemap, with real `lastmod` dates, and reference the sitemap in `robots.txt`. Done when every sitemap URL answers 200 and is its own canonical.
6. Add structured data as JSON-LD only for types Google still shows (see `## Gotchas`) and only for what is visible on the page; Open Graph and Twitter cards with absolute image URLs at 1200×630. Done when the Rich Results Test reports the type as valid, or you have listed it for the user to test on the deployed URL.
7. Run the commands of `## Verify`, and put the before and after values in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- FAQ rich results are no longer shown in Google Search (since 2026-05-07; the docs were removed on 2026-06-15). Google retired course info, estimated salary, learning video, special announcement, vehicle listing, ClaimReview and book actions in 2025, practice problem in January 2026, HowTo in 2023 and the sitelinks search box in November 2024. Do not add these types for rich results; keep existing markup only if something else uses it.
- AI Overviews and AI Mode need nothing special: Google says no `llms.txt`, no AI text files, no special markup or Markdown, and no chunking. A page must be indexed and eligible for a snippet. `nosnippet` and `max-snippet` also limit what AI Overviews and AI Mode may use; a site-wide opt-out lives in Search Console. Keeping an `llms.txt` for other services neither helps nor harms Search.
- `Google-Extended` in `robots.txt` controls Gemini training and grounding only; it does not change inclusion or ranking in Google Search.
- `robots.txt` controls crawling, not indexing: a disallowed URL can still be indexed from links, and Google cannot see a `noindex` on a page it may not crawl. To remove a page, leave it crawlable and serve `noindex`.
- JavaScript sites: put the canonical in the server HTML and never let JavaScript change it to a different URL; a `noindex` in the initial HTML cannot be removed by JavaScript (Google may skip rendering it); pages that answer non-200 may not be rendered; a client-side "not found" view must redirect to a URL that answers 404 or add `noindex`. Links count only as `<a href>`, and routes need real paths (History API), not `#` fragments.
- Core Web Vitals are LCP (≤ 2.5 s), INP (≤ 200 ms, replaced FID in March 2024) and CLS (≤ 0.1), at the 75th percentile of field data. Lighthouse is lab data and a page-load run has no interaction, so no INP: report Total Blocking Time as its proxy and say so.
- Generating many near-identical pages for query variations is scaled content abuse under Google's spam policies, whatever tool writes them.

## Verify

- The project's own checks first (a Lighthouse CI config, e2e tests on metadata).
- `curl -s http://localhost:$PORT/<path> | grep -iE '<title|name="description"|rel="canonical"|hreflang|name="robots"|application/ld\+json'`
- `npx lighthouse http://localhost:$PORT/<path> --only-categories=seo,performance --output=json --output-path=./lighthouse.json --chrome-flags="--headless"` (or the project's Lighthouse setup).
- Rich Results Test (https://search.google.com/test/rich-results) on the deployed URL, listed for the user when the site is not public.

## Security

- Serialize JSON-LD with a JSON encoder, then replace `<` with `\u003c` (and `>` with `\u003e`, `&` with `\u0026`) before putting it in `<script type="application/ld+json">`, so content from users cannot close the tag and inject HTML. In JavaScript: `JSON.stringify(data).replace(/</g, "\\u003c").replace(/>/g, "\\u003e").replace(/&/g, "\\u0026")`; or use a framework helper that already does it.
- Build canonical, hreflang, Open Graph and sitemap URLs from the configured site origin; a URL taken from the `Host` header lets an attacker point your canonicals elsewhere through cache poisoning.
- `robots.txt` is public: listing private paths there advertises them. Protect staging and admin areas with authentication, and send `X-Robots-Tag: noindex` on staging.
- Keep private, user-specific and token-bearing URLs out of sitemaps and canonicals.

## References

- Google Search documentation updates (check for changes after `termf-verified`): https://developers.google.com/search/updates
- Structured data types Google still supports: https://developers.google.com/search/docs/appearance/structured-data/search-gallery
- AI features guidance: https://developers.google.com/search/docs/fundamentals/ai-optimization-guide ; robots meta rules: https://developers.google.com/search/docs/crawling-indexing/robots-meta-tag
