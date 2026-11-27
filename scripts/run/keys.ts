// Role keys. Devnet: derived from the ROLE_SEED secret (GitHub Actions secret, never in the repository).
// Local runs use a fixed public test seed. Only public keys are written to run.json / demo.json.
import { Keypair } from "@solana/web3.js";
import { createHmac } from "node:crypto";

export const ROLES = ["admin", "payer", "investor_rep", "verifier0", "verifier1", "verifier2", "manufacturer", "opex",
  "investorA", "investorB", "investorC"] as const;
export type Role = (typeof ROLES)[number];

export function roleKeys(seed: string): Record<Role, Keypair> {
  const out = {} as Record<Role, Keypair>;
  for (const r of ROLES) {
    const s = createHmac("sha256", seed).update(`savesail-role-${r}`).digest();
    out[r] = Keypair.fromSeed(s);
  }
  return out;
}

export function keypairFromEnv(name: string): Keypair | null {
  const v = process.env[name];
  if (!v) return null;
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(v)));
}
