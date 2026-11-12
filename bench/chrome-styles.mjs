// Dump Chrome's getComputedStyle() for every element of every page at every
// viewport, keyed by the same structural path cssdiff uses.
//
// usage: node chrome-styles.mjs <site-dir> <base> <viewports> <properties-file> <out.json>
import { createServer } from "node:http";
import { readFile, readdir, writeFile } from "node:fs/promises";
import { existsSync, readdirSync } from "node:fs";
import { extname, join, relative } from "node:path";
import { homedir } from "node:os";
import { chromium } from "playwright-core";

const [site, base, viewportArg, propsFile, out] = process.argv.slice(2);
const viewports = viewportArg.split(",").map((v) => v.split("x").map(Number));
const properties = (await readFile(propsFile, "utf8")).trim().split("\n");

async function htmlFiles(dir) {
  const out = [];
  for (const e of await readdir(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) out.push(...(await htmlFiles(p)));
    else if ([".html", ".htm"].includes(extname(p))) out.push(relative(site, p));
  }
  return out.sort();
}

function chromePath() {
  if (process.env.CHROME) return process.env.CHROME;
  const root = join(homedir(), ".cache/ms-playwright");
  const found = readdirSync(root)
    .filter((d) => d.startsWith("chromium_headless_shell-"))
    .sort((a, b) => Number(a.split("-")[1]) - Number(b.split("-")[1]))
    .map((d) => join(root, d, "chrome-headless-shell-linux64/chrome-headless-shell"))
    .filter(existsSync)
    .at(-1);
  if (!found) throw new Error("no chromium headless shell found, set CHROME");
  return found;
}

const server = createServer(async (req, res) => {
  let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  if (base && path.startsWith(base + "/")) path = path.slice(base.length);
  try {
    const body = await readFile(join(site, path));
    const type = { ".css": "text/css", ".html": "text/html" }[extname(path)] ?? "application/octet-stream";
    res.writeHead(200, { "content-type": type }).end(body);
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const origin = `http://127.0.0.1:${server.address().port}`;

const browser = await chromium.launch({ executablePath: chromePath(), args: ["--hide-scrollbars"] });
const context = await browser.newContext({ javaScriptEnabled: false });
const tab = await context.newPage();
const result = {};

for (const file of await htmlFiles(site)) {
  await tab.setViewportSize({ width: viewports[0][0], height: viewports[0][1] });
  await tab.goto(`${origin}${base}/${file}`, { waitUntil: "load" });
  await tab.waitForTimeout(300);
  result[file] = {};
  for (const [width, height] of viewports) {
    await tab.setViewportSize({ width, height });
    await tab.waitForTimeout(50);
    result[file][`${width}x${height}`] = await tab.evaluate((properties) => {
      const out = {};
      const dump = (style) => Object.fromEntries(properties.map((p) => [p, style.getPropertyValue(p)]));
      const walk = (parent, key) => {
        const seen = {};
        for (const el of parent.children) {
          const tag = el.localName;
          seen[tag] = (seen[tag] ?? 0) + 1;
          const k = `${key}/${tag}[${seen[tag]}]`;
          out[k] = dump(getComputedStyle(el));
          for (const pseudo of ["::before", "::after"]) {
            const s = getComputedStyle(el, pseudo);
            if (!["none", "normal"].includes(s.content)) out[k + pseudo] = dump(s);
          }
          walk(el, k);
        }
      };
      walk(document, "");
      return out;
    }, properties);
  }
}

await browser.close();
server.close();
await writeFile(out, JSON.stringify(result));
