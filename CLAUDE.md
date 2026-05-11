# CLAUDE.md — Solignition CLI

## Project Overview

Rust CLI for the **Solignition** protocol — a Solana DeFi lending platform that lets developers deploy programs without upfront capital. Users borrow SOL to cover deployment costs (rent + tx fees), the protocol's deployer service deploys their `.so` binary, and upon repayment the program's upgrade authority transfers to the borrower.

## Architecture

```
src/
├── main.rs        # CLI entry point, clap command definitions, command handler functions
├── config.rs      # Config file (~/.solignition/config.toml), keypair loading
├── client.rs      # HTTP client for the deployer API (upload, notify, status)
├── solana_ops.rs  # On-chain interactions: build & send transactions, parse accounts
└── display.rs     # Terminal output formatting, tables, spinners, colors
```

### Key Design Decisions

- **No Anchor client dependency** — instructions are built manually from IDL discriminators to avoid version conflicts and keep the binary small. The discriminators are hardcoded constants extracted from the IDL.
- **Raw account parsing** — on-chain account data (ProtocolConfig, Loan) is parsed from raw bytes using offset constants derived from the Anchor struct layout (8-byte discriminator prefix + fields in declaration order).
- **Async with tokio** but `solana-client::RpcClient` is synchronous — the async boundary is at the command handler level.

## On-Chain Program

- **Program ID**: `HVzpjSxwECnb6uY9Jnia48oJp4xrQiz5jgc5hZC5df63`
- **Framework**: Anchor 0.31
- **IDL source**: `../anchor/target/types/solignition.ts` (TypeScript IDL)
- **IDL JSON**: loaded by the deployer service at runtime

### PDA Seeds

| Account          | Seeds                                          |
|------------------|------------------------------------------------|
| ProtocolConfig   | `"config"`                                     |
| Vault            | `"vault"`                                      |
| AdminPda         | `"admin"`                                      |
| AuthorityPda     | `"authority"`                                  |
| Loan             | `"loan"` + loan_id (u64 LE) + borrower pubkey  |
| DepositorRecord  | `"depositor"` + depositor pubkey               |
| EventAuthority   | `"__event_authority"`                          |

### Instructions Used by CLI

| Instruction    | Discriminator                          | Args                                              |
|----------------|----------------------------------------|----------------------------------------------------|
| `requestLoan`  | `[120, 2, 7, 7, 1, 219, 235, 187]`    | principal: u64, duration: i64, interestRateBps: u16, adminFeeBps: u16 |
| `repayLoan`    | `[224, 93, 144, 77, 61, 17, 137, 54]` | loanId: u64                                        |

### Loan States (enum byte values)

| Value | State                   |
|-------|-------------------------|
| 0     | Active                  |
| 1     | Repaid                  |
| 2     | Recovered               |
| 3     | Pending                 |
| 4     | RepaidPendingTransfer   |
| 5     | Reclaimed               |

### Account Data Layout — ProtocolConfig

```
Offset  Size  Field
0       8     Anchor discriminator
8       32    admin (Pubkey)
40      32    treasury (Pubkey)
72      32    deployer (Pubkey)
104     2     admin_fee_split_bps (u16)
106     2     default_interest_rate_bps (u16)
108     2     default_admin_fee_bps (u16)
110     8     total_loans_outstanding (u64)
118     8     total_shares (u64)
126     8     total_yield_distributed (u64)
134     8     loan_counter (u64)
142     1     is_paused (bool)
143     1     bump (u8)
```

### Account Data Layout — Loan

```
Offset  Size  Field
0       8     Anchor discriminator
8       8     loan_id (u64)
16      32    borrower (Pubkey)
48      32    program_pubkey (Pubkey)
80      8     principal (u64)
88      8     duration (i64)
96      2     interest_rate_bps (u16)
98      2     admin_fee_bps (u16)
100     8     admin_fee_paid (u64)
108     8     start_ts (i64)
116     1     state (enum tag)
117     32    authority_pda (Pubkey)
149+    ...   Optional fields (repaid_ts, recovered_ts, interest_paid, etc.)
```

> ⚠️ **These offsets are derived from the IDL type definitions assuming no padding. If Anchor adds alignment padding on any version change, these need to be verified against actual on-chain data using `solana account <PDA> --output json`.**

## Deployer API

The CLI communicates with a Node.js/Express deployer service. Base URL is configured via `api_url`.

### Endpoints Used

| Method | Path                              | Purpose                              |
|--------|-----------------------------------|--------------------------------------|
| POST   | `/upload`                         | Upload .so binary (multipart form)   |
| POST   | `/notify-loan`                    | Trigger deployment after loan tx     |
| POST   | `/notify-repaid`                  | Trigger authority transfer after repay |
| GET    | `/uploads/:fileId`                | Get upload info                      |
| GET    | `/uploads/borrower/:pubkey`       | List uploads for a wallet            |
| GET    | `/deployments/:loanId`            | Get deployment status                |
| GET    | `/deployments/borrower/:pubkey`   | List deployments for a wallet        |
| GET    | `/health`                         | Service health check                 |

### Upload Request

```
POST /upload
Content-Type: multipart/form-data

Fields:
  - file: the .so binary
  - borrower: wallet pubkey string
```

### Notify Loan Request

```json
{
  "signature": "tx_signature",
  "borrower": "pubkey",
  "loanId": "0",
  "fileId": "abc123"
}
```

## Build & Test

```bash
cargo build --release
cargo test
cargo clippy
```

The binary outputs to `target/release/solignition`.

## Common Tasks

### Adding a new command

1. Add variant to `Commands` enum in `main.rs`
2. Add match arm calling `cmd_<name>` async function
3. Implement the function — use `client.rs` for API calls, `solana_ops.rs` for on-chain

### Adding a new on-chain instruction

1. Get the 8-byte discriminator from the IDL (`instructions[n].discriminator`)
2. Add it as a constant in `solana_ops.rs`
3. Build the accounts list matching the IDL's `accounts` array (order matters)
4. Serialize args in order: discriminator bytes + each arg in little-endian

### Verifying account layout offsets

```bash
# Fetch raw account data
solana account <PDA_ADDRESS> --output json --url <RPC_URL>

# Compare the base64-decoded bytes against expected offsets
```

## Dependencies

- `solana-sdk` / `solana-client` 2.2 — match to your cluster version
- `clap` 4 — CLI parsing with derive macros
- `reqwest` 0.12 — HTTP with multipart upload support
- `colored` / `indicatif` / `dialoguer` — terminal UX
- No Anchor runtime dependency

## Config Precedence

CLI flags > Environment variables > Config file (`~/.solignition/config.toml`) > Defaults

| Setting    | Env Var                  | CLI Flag       | Default                                          | 
|------------|--------------------------|----------------|--------------------------------------------------|
| API URL    | `SOLIGNITION_API_URL`    | `--api-url`    | `http://localhost:3000`                          | 
| RPC URL    | `SOLANA_RPC_URL`         | `--rpc-url`    | `https://api.devnet.solana.com`                  |
| Keypair    | `SOLIGNITION_KEYPAIR`    | `--keypair`    | `~/.config/solana/id.json`                       |
| Program ID | `SOLIGNITION_PROGRAM_ID` | `--program-id` | `HVzpjSxwECnb6uY9Jnia48oJp4xrQiz5jgc5hZC5df63`   |