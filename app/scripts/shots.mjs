// Screenshots of the 5 pages (desktop 1280 and 390 px) for review: node scripts/shots.mjs OUT_DIR DEST_DIR
import { createServer } from "node:http";
import { existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { extname, join } from "node:path";
import * as chromeLauncher from "chrome-launcher";
import puppeteer from "puppeteer-core";
const [out = "out", dest = "shots"] = process.argv.slice(2);
const T = { ".html": "text/html; charset=utf-8", ".js": "text/javascript", ".css": "text/css", ".svg": "image/svg+xml" };
const s = createServer((q, r) => { let f = join(out, decodeURIComponent(q.url.split("?")[0])); if (existsSync(f) && statSync(f).isDirectory()) f = join(f, "index.html");
  if (!existsSync(f)) { r.writeHead(404).end(); return; } r.writeHead(200, { "content-type": T[extname(f)] ?? "application/octet-stream" }); r.end(readFileSync(f)); }).listen(0);
const port = s.address().port;
const c = await chromeLauncher.launch({ chromeFlags: ["--headless=new", "--no-sandbox"] });
const b = await puppeteer.connect({ browserURL: `http://127.0.0.1:${c.port}` });
const p = await b.newPage();
mkdirSync(dest, { recursive: true });
for (const [w, tag] of [[1280, "desktop"], [390, "m390"]]) {
  await p.setViewport({ width: w, height: 900, deviceScaleFactor: 1 });
  for (const pg of ["", "invest/", "schedule/", "verify/", "settle/"]) {
    await p.goto(`http://127.0.0.1:${port}/${pg}`, { waitUntil: "networkidle0" });
    await p.screenshot({ path: join(dest, `${tag}_${pg.replace("/", "") || "overview"}.png`), fullPage: true });
  }
}
await b.disconnect(); await c.kill(); s.close();
