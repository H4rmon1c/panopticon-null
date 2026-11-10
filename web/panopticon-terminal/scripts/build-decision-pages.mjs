import { readFile, mkdir, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { validateSnapshot } from "../api.js";

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character]);
}

function replaceOne(html, pattern, replacement, label) {
  const matches = [...html.matchAll(pattern)];
  if (matches.length !== 1) throw new Error(`Decision template needs exactly one ${label}`);
  return html.replace(pattern, () => replacement);
}

export function buildDecisionPages(snapshot, template, siteUrl) {
  validateSnapshot(snapshot);
  const origin = new URL(siteUrl);
  if (origin.protocol !== "https:" || origin.href !== `${origin.origin}/`) {
    throw new Error("Site URL must be an HTTPS origin");
  }
  const pages = [];
  const seen = new Set();
  for (const decision of snapshot.entities.filter((entity) => entity.type === "DECISION")) {
    const slug = decision.slug;
    if (!/^[a-z0-9][a-z0-9-]*$/.test(slug ?? "") || seen.has(slug)) {
      throw new Error("Decision has an unsafe or duplicate slug");
    }
    seen.add(slug);
    const expectedUrl = `${origin.origin}/decisions/${slug}.html`;
    const legacyUrl = `${origin.origin}/decision.html?decision=co-colorado-springs-25-581-2025-11-25`;
    if (slug === "co-colorado-springs-25-581-2025-11-25" && decision.share_url === legacyUrl) continue;
    if (decision.share_url !== expectedUrl) throw new Error(`Decision ${slug} lacks its case-specific share URL`);

    const title = escapeHtml(`${decision.name} | PANOPTICON.FAIL`);
    const summary = escapeHtml(String(decision.summary ?? decision.description ?? decision.name).replace(/\s+/g, " ").trim().slice(0, 280));
    const url = escapeHtml(expectedUrl);
    let html = template;
    for (const [pattern, replacement, label] of [
      [/<meta name="description" content="[^"]*"\s*\/>/g, `<meta name="description" content="${summary}" />`, "description"],
      [/<meta property="og:title" content="[^"]*"\s*\/>/g, `<meta property="og:title" content="${title}" />`, "og:title"],
      [/<meta property="og:description" content="[^"]*"\s*\/>/g, `<meta property="og:description" content="${summary}" />`, "og:description"],
      [/<meta property="og:url" content="[^"]*"\s*\/>/g, `<meta property="og:url" content="${url}" />`, "og:url"],
      [/<meta name="twitter:title" content="[^"]*"\s*\/>/g, `<meta name="twitter:title" content="${title}" />`, "twitter:title"],
      [/<meta name="twitter:description" content="[^"]*"\s*\/>/g, `<meta name="twitter:description" content="${summary}" />`, "twitter:description"],
      [/<link rel="canonical" href="[^"]*"\s*\/>/g, `<link rel="canonical" href="${url}" />`, "canonical URL"],
      [/<title>[^<]*<\/title>/g, `<title>${title}</title>`, "title"],
    ]) html = replaceOne(html, pattern, replacement, label);
    html = html.replaceAll('href="./decision.css"', 'href="../decision.css"')
      .replaceAll('src="./decision.js"', 'src="../decision.js"')
      .replaceAll('href="./index.html"', 'href="../index.html"');
    if (!html.includes('href="../decision.css"') || !html.includes('src="../decision.js"')) {
      throw new Error("Decision template assets did not resolve from case path");
    }
    pages.push({ path: `decisions/${slug}.html`, html });
  }
  return pages;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [, , stage, siteUrl] = process.argv;
  if (!stage || !siteUrl) throw new Error("Usage: build-decision-pages.mjs STAGE SITE_ORIGIN");
  const snapshot = JSON.parse(await readFile(join(stage, "public/public-snapshot.json"), "utf8"));
  const template = await readFile(join(stage, "decision.html"), "utf8");
  const pages = buildDecisionPages(snapshot, template, siteUrl);
  await mkdir(join(stage, "decisions"), { recursive: true });
  for (const page of pages) await writeFile(join(stage, page.path), page.html, { flag: "wx" });
  console.log(`generated ${pages.length} case-specific decision page(s)`);
}
