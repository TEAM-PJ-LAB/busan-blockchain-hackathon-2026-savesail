// PRD G2 / 7.5: Lighthouse (desktop) on all 5 pages, console errors, 390px width without horizontal scroll.
// usage: node scripts/web_check.mjs OUT_DIR BASE_PATH [REPORT.json]
import { createServer } from "node:http";
import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { extname, join, normalize } from "node:path";
import * as chromeLauncher from "chrome-launcher";
import lighthouse from "lighthouse";
import desktop from "lighthouse/core/config/desktop-config.js";
import puppeteer from "puppeteer-core";

const [out = "out", base = "", reportPath] = process.argv.slice(2);
const PAGES = ["", "invest/", "schedule/", "verify/", "settle/"];
const TYPES = { ".html": "text/html; charset=utf-8", ".js": "text/javascript", ".css": "text/css", ".json": "application/json",
  ".svg": "image/svg+xml", ".ico": "image/x-icon", ".txt": "text/plain", ".woff2": "font/woff2" };

const server = createServer((req, res) => {
  let p = decodeURIComponent((req.url ?? "/").split("?")[0]);
  if (!p.startsWith(base + "/")) { res.writeHead(404).end(); return; }
  p = normalize(p.slice(base.length)).replace(/^(\.\.[/\\])+/, "");
  let f = join(out, p);
  if (existsSync(f) && statSync(f).isDirectory()) f = join(f, "index.html");
  if (!existsSync(f)) { res.writeHead(404).end(); return; }
  res.writeHead(200, { "content-type": TYPES[extname(f)] ?? "application/octet-stream", "cache-control": "public, max-age=31536000" });
  res.end(readFileSync(f));
}).listen(0);
const port = server.address().port;
const url = (pg) => `http://127.0.0.1:${port}${base}/${pg}`;

const chrome = await chromeLauncher.launch({ chromeFlags: ["--headless=new", "--no-sandbox", "--disable-gpu"] });
const results = [];
let fail = false;
for (const pg of PAGES) {
  const r = await lighthouse(url(pg), { port: chrome.port, output: "json", logLevel: "error" }, desktop);
  const c = r.lhr.categories, a = r.lhr.audits;
  const row = { page: "/" + pg, performance: c.performance.score * 100, accessibility: c.accessibility.score * 100,
    best_practices: c["best-practices"].score * 100, lcp_ms: Math.round(a["largest-contentful-paint"].numericValue),
    console_errors: a["errors-in-console"].details?.items?.length ?? 0 };
  const ok = row.performance >= 90 && row.accessibility >= 95 && row.best_practices >= 95 && row.lcp_ms <= 3000 && row.console_errors === 0;
  if (!ok) {
    fail = true;
    const bad = Object.values(a).filter((x) => x.score !== null && x.score < 1 && x.scoreDisplayMode !== "informative").map((x) => x.id);
    row.failing_audits = bad;
  }
  results.push(row);
  console.log(`${ok ? "PASS" : "FAIL"} ${JSON.stringify(row)}`);
}

// 390px: no horizontal scroll, no console errors, every page renders its heading
const browser = await puppeteer.connect({ browserURL: `http://127.0.0.1:${chrome.port}` });
const page = await browser.newPage();
await page.setViewport({ width: 390, height: 844, deviceScaleFactor: 3, isMobile: true, hasTouch: true });
const errors = [];
page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
page.on("pageerror", (e) => errors.push(String(e)));
for (const pg of PAGES) {
  const before = errors.length;
  await page.goto(url(pg), { waitUntil: "networkidle0" });
  const m = await page.evaluate(() => ({ sw: document.documentElement.scrollWidth, cw: document.documentElement.clientWidth,
    h1: document.querySelector("h1")?.textContent ?? "" }));
  const ok = m.sw <= m.cw && m.h1.length > 0 && errors.length === before;
  if (!ok) fail = true;
  results.push({ page: "/" + pg, width: 390, scroll_width: m.sw, client_width: m.cw, h1: m.h1, console_errors: errors.slice(before) });
  console.log(`${ok ? "PASS" : "FAIL"} 390px /${pg} scrollWidth ${m.sw} <= ${m.cw}, errors ${errors.length - before}`);
}
await browser.disconnect();
await chrome.kill();
server.close();
if (reportPath) writeFileSync(reportPath, JSON.stringify(results, null, 1));
process.exit(fail ? 1 : 0);
