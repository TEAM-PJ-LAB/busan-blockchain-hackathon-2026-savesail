// PRD G2 / 7.5: Lighthouse (desktop) on all 5 pages, console errors, 390px width without horizontal scroll.
// usage: node scripts/web_check.mjs OUT_DIR BASE_PATH [REPORT.json]
import { writeFileSync } from "node:fs";
import * as chromeLauncher from "chrome-launcher";
import lighthouse from "lighthouse";
import desktop from "lighthouse/core/config/desktop-config.js";
import { computeMedianRun } from "lighthouse/core/lib/median-run.js";
import puppeteer from "puppeteer-core";
import { PAGES, serve } from "./static_server.mjs";

const [out = "out", base = "", reportPath] = process.argv.slice(2);
const { server, url } = serve(out, base);

// Lighthouse scores vary run to run, most on shared CI machines; the Lighthouse docs advise several runs and the
// median (docs/variability.md: 5 runs about halve the variance). One discarded warm-up run, then the median of 5
// per page by Lighthouse's own computeMedianRun. The thresholds are unchanged.
const RUNS = 5;
const chrome = await chromeLauncher.launch({ chromeFlags: ["--headless=new", "--no-sandbox", "--disable-gpu"] });
const lh = (u) => lighthouse(u, { port: chrome.port, output: "json", logLevel: "error" }, desktop);
await lh(url(PAGES[0]));
const results = [];
let fail = false;
for (const pg of PAGES) {
  const runs = [];
  for (let i = 0; i < RUNS; i++) runs.push((await lh(url(pg))).lhr);
  const lhr = computeMedianRun(runs);
  const c = lhr.categories, a = lhr.audits;
  const row = { page: "/" + pg, performance: c.performance.score * 100, accessibility: c.accessibility.score * 100,
    best_practices: c["best-practices"].score * 100, lcp_ms: Math.round(a["largest-contentful-paint"].numericValue),
    console_errors: Math.max(...runs.map((x) => x.audits["errors-in-console"].details?.items?.length ?? 0)),
    performance_runs: runs.map((x) => Math.round(x.categories.performance.score * 100)) };
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
