# P1 test fixtures

`tests/p1.rs` loads these two programs into LiteSVM at their published addresses (PRD appendix B).
Both are MIT-licensed (Copyright (c) 2025 Solana Foundation) and built from source without changes.

| File | Program | Address | Source commit | sha256 |
|---|---|---|---|---|
| `token_acl.so` | Token ACL (sRFC 37) | `TACLkU6CiCdkQN2MjoyDkVg2yAH9zkxiHDsiztQ52TP` | [solana-foundation/token-acl@c6941ce](https://github.com/solana-foundation/token-acl/tree/c6941cec9602574ecd3cd24db843fe6f85fa6600) | `9a6a8a99e70ee2bff198c51d1be0605215ac1646a6e6acb4415c19cdc6e13cf9` |
| `token_acl_gate.so` | ABL gate (allow/block lists) | `GATEzzqxhJnsWF6vHRsgtixxSB8PaQdcqGEVTEHWiULz` | [solana-foundation/token-acl-gate@86ee7ba](https://github.com/solana-foundation/token-acl-gate/tree/86ee7ba918f9e163b445b0f2c79de8e92d1c0bad) | `539babf0c9d3874913e2a7d3eec0262bd50975a280ac46fd96b1baad942152f9` |

Rebuild (cargo-build-sbf 4.6.0, platform-tools v1.58):

```sh
git clone https://github.com/solana-foundation/token-acl && git -C token-acl checkout c6941ce
(cd token-acl/program && cargo build-sbf)                                   # target/deploy/token_acl.so
git clone https://github.com/solana-foundation/token-acl-gate && git -C token-acl-gate checkout 86ee7ba
(cd token-acl-gate/program && RUSTUP_TOOLCHAIN=stable cargo build-sbf)      # target/deploy/token_acl_gate_program.so
```

The gate pins Rust 1.97.1 in `rust-toolchain.toml`; `RUSTUP_TOOLCHAIN=stable` only selects the host
toolchain that drives `cargo build-sbf`, the program itself is compiled by the platform tools.

`.github/workflows/devnet-p1.yml` also dumps both programs from devnet and runs `tests/p1.rs` against
those binaries (`P1_FIXTURES=<dir>`), so the tests cover the deployed code as well.
