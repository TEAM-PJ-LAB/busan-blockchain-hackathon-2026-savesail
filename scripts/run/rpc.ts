// Rate-limited JSON-RPC fetch for the public devnet endpoint (PRD 12.1, G1). The public endpoint allows 100 requests
// per 10 s per IP, 40 per 10 s for a single method and 40 connections per 10 s (solana.com/docs/references/clusters);
// a full run (about 525 transactions) must stay under them instead of failing on 429. Requests wait for a slot in a sliding
// 10-second window (total and per method, with headroom), and a 429 that still comes back is retried with backoff.
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export interface Limits {
  total: number;
  perMethod: number;
}

export function rateLimitedFetch(limits: Limits = { total: 90, perMethod: 36 }, maxRetries = 10, WINDOW_MS = 10_000) {
  // Each request holds its slot from the moment it is sent until WINDOW_MS after its response came back: the server
  // counts a request when it arrives, which is between those two times, so this never runs ahead of the server.
  type Slot = { t: number };
  let all: Slot[] = [];
  const by = new Map<string, Slot[]>();
  let chain = Promise.resolve(); // one waiter at a time keeps the window accounting exact
  const acquire = (method: string) => {
    const slot: Slot = { t: Infinity };
    chain = chain.then(async () => {
      for (;;) {
        const now = Date.now();
        all = all.filter((x) => now - x.t < WINDOW_MS);
        const q = (by.get(method) ?? []).filter((x) => now - x.t < WINDOW_MS);
        by.set(method, q);
        if (all.length < limits.total && q.length < limits.perMethod) {
          all.push(slot);
          q.push(slot);
          return;
        }
        const wait = (s: Slot[], cap: number) =>
          s.length < cap ? 0 : Math.min(...s.map((x) => x.t)) + WINDOW_MS - now; // Infinity while one is in flight
        await sleep(Math.min(Math.max(wait(all, limits.total), wait(q, limits.perMethod), 10), 50));
      }
    });
    return chain.then(() => slot);
  };
  return async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    let method = "?";
    try {
      const b = JSON.parse(String(init?.body ?? ""));
      method = Array.isArray(b) ? "batch" : String(b.method);
    } catch {
      /* not a JSON-RPC body */
    }
    for (let attempt = 0; ; attempt++) {
      const slot = await acquire(method);
      let res: Response;
      try {
        res = await fetch(input, init);
      } finally {
        slot.t = Date.now();
      }
      if (res.status !== 429 || attempt >= maxRetries) return res;
      const after = Number(res.headers.get("retry-after"));
      await sleep(after > 0 ? after * 1000 : Math.min(500 * 2 ** attempt, 16_000));
    }
  };
}
