// Writes the devnet program keypair derived from ROLE_SEED to the given path and prints only the program id.
// The workflow compiles this id into the program before deploying, so no key file is ever stored.
import { Keypair } from "@solana/web3.js";
import { createHmac } from "node:crypto";
import { writeFileSync } from "node:fs";

const seed = process.env.ROLE_SEED;
if (!seed) throw new Error("ROLE_SEED missing");
const k = Keypair.fromSeed(createHmac("sha256", seed).update("savesail-program-id").digest());
writeFileSync(process.argv[2], JSON.stringify(Array.from(k.secretKey)), { mode: 0o600 });
console.log(k.publicKey.toBase58());
