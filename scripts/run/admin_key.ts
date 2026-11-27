// Writes the admin (deployer) keypair derived from ROLE_SEED to the given path, and prints only the public key.
import { writeFileSync } from "node:fs";
import { roleKeys } from "./keys.js";

const k = roleKeys(process.env.ROLE_SEED ?? "")["admin"];
if (!process.env.ROLE_SEED) throw new Error("ROLE_SEED missing");
writeFileSync(process.argv[2], JSON.stringify(Array.from(k.secretKey)), { mode: 0o600 });
console.log(k.publicKey.toBase58());
