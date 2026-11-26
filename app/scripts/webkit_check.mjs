// PRD 7.5 ④: the 5 pages in WebKit (the engine of desktop Safari) at desktop width and at 390px:
// no console or page errors, no horizontal scroll, heading and main content rendered.
// usage: node scripts/webkit_check.mjs OUT_DIR BASE_PATH   (BROWSER=chromium CHROME_PATH=... runs the same checks in Chrome)
import { chromium, webkit } from "playwright";
import { PAGES, serve } from "./static_server.mjs";

const [out = "out", base = ""] = process.argv.slice(2);
const { server, url } = serve(out, base);
const browser = process.env.BROWSER === "chromium"
  ? await chromium.launch({ executablePath: process.env.CHROME_PATH }) : await webkit.launch();
const VIEWS = [{ name: "desktop 1440", viewport: { width: 1440, height: 900 } },
  { name: "390px", viewport: { width: 390, height: 844 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true }];
let fail = false;
for (const v of VIEWS) {
  const ctx = await browser.newContext({ viewport: v.viewport, deviceScaleFactor: v.deviceScaleFactor, isMobile: v.isMobile, hasTouch: v.hasTouch });
  const page = await ctx.newPage();
  const errors = [];
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  page.on("pageerror", (e) => errors.push(String(e)));
  for (const pg of PAGES) {
    const before = errors.length;
    const res = await page.goto(url(pg), { waitUntil: "networkidle" });
    const m = await page.evaluate(() => ({ sw: document.documentElement.scrollWidth, cw: document.documentElement.clientWidth,
      h1: document.querySelector("h1")?.textContent ?? "", main: document.querySelector("main")?.innerText.length ?? 0 }));
    const ok = res?.ok() && m.sw <= m.cw && m.h1.length > 0 && m.main > 200 && errors.length === before;
    if (!ok) fail = true;
    console.log(`${ok ? "PASS" : "FAIL"} ${browser.browserType().name()} ${v.name} /${pg} status ${res?.status()} scrollWidth ${m.sw} <= ${m.cw}, main ${m.main} chars, errors ${JSON.stringify(errors.slice(before))}`);
  }
  await ctx.close();
}
await browser.close();
server.close();
process.exit(fail ? 1 : 0);
