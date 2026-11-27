// Serves a Next.js static export (OUT_DIR) under BASE_PATH on a free local port, like GitHub Pages does.
import { createServer } from "node:http";
import { existsSync, readFileSync, statSync } from "node:fs";
import { extname, join, normalize } from "node:path";

const TYPES = { ".html": "text/html; charset=utf-8", ".js": "text/javascript", ".css": "text/css", ".json": "application/json",
  ".svg": "image/svg+xml", ".ico": "image/x-icon", ".txt": "text/plain", ".woff2": "font/woff2" };

export function serve(out, base) {
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
  return { server, url: (pg) => `http://127.0.0.1:${port}${base}/${pg}` };
}

export const PAGES = ["", "invest/", "schedule/", "verify/", "settle/"];
