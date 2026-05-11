# 🔥 Solignition CLI

A command-line tool for deploying Solana programs via the **Solignition** lending protocol — deploy without upfront capital.

## Overview

Solignition lets you deploy Solana programs by taking a short-term loan to cover deployment costs (rent + transaction fees). This CLI handles the full workflow:

1. **Upload** your compiled `.so` binary
2. **Deploy** — request a loan and get your program deployed on-chain
3. **Repay** the loan to claim full program ownership

## Installation

Pick whichever channel fits your workflow.

### crates.io (Rust)

```bash
cargo install solignition-cli
```

### npm / yarn

```bash
npm install -g solignition
# or
yarn global add solignition
```

The npm package ships precompiled binaries for Linux (x64 / arm64), macOS (x64 / arm64), and Windows (x64) — no Rust toolchain required.

### Prebuilt binaries

Grab the archive for your platform from the [latest GitHub Release](https://github.com/ORG/solignition-cli/releases/latest), extract, and put `solignition` on your `PATH`.

### From source

```bash
git clone https://github.com/ORG/solignition-cli
cd solignition-cli
cargo build --release
# Binary at target/release/solignition
```

### Prerequisites (source builds only)

- Rust 1.75+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Solana CLI tools (for keypair management)
- A Solana wallet with some SOL for transaction fees

## Quick Start

```bash
# 1. Initialize configuration
solignition init

# 2. Check your wallet
solignition wallet

# 3. Upload your program binary
solignition upload target/deploy/my_program.so

# 4. Deploy (uploads + requests loan + deploys in one step)
solignition deploy --file target/deploy/my_program.so

# 5. Check deployment status
solignition status <LOAN_ID>

# 6. Repay loan when ready to claim ownership
solignition repay <LOAN_ID>
```

## Commands

### `solignition init`

Interactive setup wizard. Configures API URL, RPC endpoint, keypair path, and program ID.

```bash
solignition init
solignition init --api-url https://api.solignition.ngrok.app --rpc-url https://api.devnet.solana.com
```

### `solignition config`

Display current configuration.

### `solignition upload <FILE>`

Upload a compiled Solana program (`.so` file) to the deployer service. Returns a **File ID** and estimated deployment cost.

```bash
solignition upload target/deploy/my_program.so
```

### `solignition deploy`

Request a deployment loan and deploy your program. Can either reference a previous upload or upload-and-deploy in one step.

```bash
# Using a previous upload
solignition deploy --file-id abc123def456

# Upload and deploy in one step
solignition deploy --file target/deploy/my_program.so

# Customize loan terms
solignition deploy --file my_program.so --duration 1209600 --interest-rate-bps 500

# Skip confirmation prompt
solignition deploy --file my_program.so -y
```

**Options:**
| Flag | Description | Default |
|------|-------------|---------|
| `--file-id` | File ID from previous upload | — |
| `--file` | Path to .so file | — |
| `--duration` | Loan duration in seconds | 604800 (7 days) |
| `--interest-rate-bps` | Interest rate in basis points | Protocol default |
| `--admin-fee-bps` | Admin fee in basis points | Protocol default |
| `-y, --yes` | Skip confirmation | false |

### `solignition repay <LOAN_ID>`

Repay your loan and receive program upgrade authority.

```bash
solignition repay 0
solignition repay 0 -y  # Skip confirmation
```

### `solignition status <LOAN_ID>`

Check the deployment status for a specific loan.

### `solignition uploads`

List all your uploaded binaries.

### `solignition loans`

List all your deployment loans and their statuses.

### `solignition health`

Check if the deployer service is running.

### `solignition wallet`

Show your wallet address and SOL balance.

### `solignition protocol-info`

Fetch and display on-chain protocol configuration (fees, rates, counters).

## Configuration

Configuration is stored at `~/.solignition/config.toml`:

```toml
api_url = "http://localhost:3000"
rpc_url = "https://api.devnet.solana.com"
keypair_path = "/home/user/.config/solana/id.json"
program_id = "HVzpjSxwECnb6uY9Jnia48oJp4xrQiz5jgc5hZC5df63"
```

> **`api_url` must be set before deploying.** The default points at `localhost:3000` for development. Run `solignition init` and supply the deployer service URL you're using (or [self-host one](#self-hosting-the-deployer)).

### Environment Variables

All config values can be overridden via environment variables:

| Variable | Description |
|----------|-------------|
| `SOLIGNITION_API_URL` | Deployer API endpoint |
| `SOLANA_RPC_URL` | Solana RPC endpoint |
| `SOLIGNITION_KEYPAIR` | Path to wallet keypair |
| `SOLIGNITION_PROGRAM_ID` | Solignition program ID |

### CLI Flags

Global flags override both config file and environment variables:

```bash
solignition --api-url https://api.example.com --rpc-url https://rpc.example.com status 0
```

## Typical Workflow

```
┌──────────┐     ┌──────────────┐     ┌─────────────┐     ┌──────────┐
│  Upload   │────▶│ Request Loan │────▶│   Deploy    │────▶│  Repay   │
│  .so file │     │  on-chain    │     │  (auto)     │     │  + claim │
└──────────┘     └──────────────┘     └─────────────┘     └──────────┘
     │                   │                    │                  │
     ▼                   ▼                    ▼                  ▼
  File ID          Loan created        Program live on      Authority
  + cost est.      + SOL disbursed     Solana (deployer    transferred
                                       holds authority)    to borrower
```

## Loan States

| State | Description |
|-------|-------------|
| `active` | Loan is active, program deployed |
| `pending` | Loan created, awaiting deployment |
| `repaid` | Loan repaid, authority transfer pending |
| `repaidPendingTransfer` | Repaid, authority being transferred |
| `recovered` | Loan expired, program recovered |
| `reclaimed` | SOL reclaimed from recovered program |

## Networks

- **Default: Devnet.** Safe for first-time use and program testing.
- **Mainnet-beta:** override with `--rpc-url https://api.mainnet-beta.solana.com` (or the URL of your mainnet RPC provider — Helius, QuickNode, Triton, etc.). The CLI detects mainnet via genesis hash and inserts an extra confirmation prompt before any signing action. `--yes` skips it for automation.
- **Localnet:** override with `--rpc-url http://127.0.0.1:8899`.

## Self-hosting the deployer

The CLI talks to the Solignition deployer service over HTTP. To run your own, see the deployer repo (sibling directory `../deployer` in the Solignition monorepo) — it ships as a Node/TypeScript service that handles uploads, on-chain deployment, repayment-driven authority transfer, and expired-loan recovery.

## License

MIT
