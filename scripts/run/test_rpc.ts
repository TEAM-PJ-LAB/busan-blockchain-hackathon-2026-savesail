// Test of the devnet rate limiter (rpc.ts) against a local server that enforces the public endpoint's rules on a
// 1-second window: no request may be refused for rate, and a 429 the server does send is retried until it succeeds.
import { createServer } from "node:http";
import { rateLimitedFetch } from "./rpc.js";

const WIN = 1000, TOTAL = 10, PER = 4; // server rules per window
const seen: { t: number; m: string }[] = [];
let refused = 0, forced = 2;
const server = createServer((req, res) => {
  let body = "";
  req.on("data", (c) => (body += c));
  req.on("end", () => {
    const m = JSON.parse(body).method, now = Date.now();
    const win = seen.filter((x) => now - x.t < WIN);
    if (forced > 0 && m === "flaky") { forced--; res.writeHead(429).end(); return; }
    if (win.length >= TOTAL || win.filter((x) => x.m === m).length >= PER) { refused++; res.writeHead(429).end(); return; }
    seen.push({ t: now, m });
    res.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify({ jsonrpc: "2.0", id: 1, result: m }));
  });
}).listen(0);
const url = `http://127.0.0.1:${(server.address() as any).port}`;
const f = rateLimitedFetch({ total: 8, perMethod: 3 }, 10, WIN);
const methods = ["getSlot", "sendTransaction", "getSignatureStatuses", "getAccountInfo"];
const t0 = Date.now();
const calls = Array.from({ length: 48 }, (_, i) => f(url, { method: "POST", body: JSON.stringify({ method: methods[i % 4] }) }));
calls.push(f(url, { method: "POST", body: JSON.stringify({ method: "flaky" }) }));
const res = await Promise.all(calls);
const ok = res.filter((r) => r.status === 200).length;
const dt = Date.now() - t0;
server.close();
console.log(`${ok}/${res.length} ok, ${refused} refused for rate, ${dt} ms`);
// 49 requests at 8 per second need at least 5 full windows
if (ok !== res.length || refused !== 0 || dt < 5 * WIN) process.exit(1);
