use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};
use solana_system_interface::program as system_program;
use std::str::FromStr;

use crate::config::Config;

// ─── Seeds ───────────────────────────────────────────────────────────────────

const VAULT_SEED: &[u8] = b"vault";
const CONFIG_SEED: &[u8] = b"config";
const LOAN_SEED: &[u8] = b"loan";
const ADMIN_SEED: &[u8] = b"admin";
const EVENT_AUTHORITY_SEED: &[u8] = b"__event_authority";

// ─── Instruction Discriminators (from IDL) ───────────────────────────────────

const REQUEST_LOAN_DISC: [u8; 8] = [120, 2, 7, 7, 1, 219, 235, 187];
const REPAY_LOAN_DISC: [u8; 8] = [224, 93, 144, 77, 61, 17, 137, 54];

// ─── Network Identification ──────────────────────────────────────────────────

/// Solana mainnet-beta cluster genesis hash. Used to detect when the user's
/// configured RPC points to mainnet so we can gate signing actions behind an
/// extra confirmation. Same value the official Solana CLI uses.
const MAINNET_GENESIS_HASH: &str = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d";

// ─── Account Layout Constants ────────────────────────────────────────────────

/// Offset into ProtocolConfig for loan_counter (u64)
/// Layout: 8 discriminator + 32 admin + 32 treasury + 32 deployer +
///         2 adminFeeSplitBps + 2 defaultInterestRateBps + 2 defaultAdminFeeBps +
///         8 totalLoansOutstanding + 8 totalShares + 8 totalYieldDistributed = 134
const PROTOCOL_LOAN_COUNTER_OFFSET: usize = 134;

/// Offset for default_interest_rate_bps (u16) in ProtocolConfig
/// 8 + 32 + 32 + 32 + 2 = 106
const PROTOCOL_DEFAULT_INTEREST_OFFSET: usize = 106;

/// Offset for default_admin_fee_bps (u16)
/// 106 + 2 = 108
const PROTOCOL_DEFAULT_ADMIN_FEE_OFFSET: usize = 108;

/// Offset for is_paused (bool)
/// 134 + 8 (loanCounter) = 142
const PROTOCOL_IS_PAUSED_OFFSET: usize = 142;

/// Offset for admin_fee_split_bps (u16)
/// 8 + 32 + 32 + 32 = 104
const PROTOCOL_ADMIN_FEE_SPLIT_OFFSET: usize = 104;

// Loan account offsets
const LOAN_DISCRIMINATOR_SIZE: usize = 8;
const LOAN_ID_OFFSET: usize = LOAN_DISCRIMINATOR_SIZE; // u64
const LOAN_BORROWER_OFFSET: usize = LOAN_ID_OFFSET + 8; // pubkey 32
const LOAN_PROGRAM_PUBKEY_OFFSET: usize = LOAN_BORROWER_OFFSET + 32; // pubkey 32
const LOAN_PRINCIPAL_OFFSET: usize = LOAN_PROGRAM_PUBKEY_OFFSET + 32; // u64
const LOAN_DURATION_OFFSET: usize = LOAN_PRINCIPAL_OFFSET + 8; // i64
const LOAN_INTEREST_RATE_OFFSET: usize = LOAN_DURATION_OFFSET + 8; // u16
const LOAN_ADMIN_FEE_BPS_OFFSET: usize = LOAN_INTEREST_RATE_OFFSET + 2; // u16
const LOAN_ADMIN_FEE_PAID_OFFSET: usize = LOAN_ADMIN_FEE_BPS_OFFSET + 2; // u64
const LOAN_START_TS_OFFSET: usize = LOAN_ADMIN_FEE_PAID_OFFSET + 8; // i64
const LOAN_STATE_OFFSET: usize = LOAN_START_TS_OFFSET + 8; // enum (1 byte tag)

// ─── Types ───────────────────────────────────────────────────────────────────

pub struct ProtocolConfigInfo {
    pub admin: Pubkey,
    pub treasury: Pubkey,
    pub deployer: Pubkey,
    pub admin_fee_split_bps: u16,
    pub default_interest_rate_bps: u16,
    pub default_admin_fee_bps: u16,
    pub total_loans_outstanding: u64,
    pub total_shares: u64,
    pub total_yield_distributed: u64,
    pub loan_counter: u64,
    pub is_paused: bool,
}

// Some fields here aren't read on every code path today but are retained for
// future commands, Debug output, and parity with the on-chain account layout.
#[allow(dead_code)]
pub struct LoanInfo {
    pub loan_id: u64,
    pub borrower: Pubkey,
    pub program_pubkey: Pubkey,
    pub principal: u64,
    pub duration: i64,
    pub interest_rate_bps: u16,
    pub admin_fee_bps: u16,
    pub start_ts: i64,
    pub state: String,
    pub interest_amount: u64,
    pub total_repayment: u64,
}

impl LoanInfo {
    /// True when the loan is on-chain `active` but past its due date,
    /// evaluated against the **chain's** Unix timestamp (Clock sysvar).
    /// Wall-clock time can drift from the chain on localnet, which is
    /// why the caller passes in `now_ts` from `fetch_chain_timestamp`.
    pub fn is_expired_at(&self, now_ts: i64) -> bool {
        self.state == "active"
            && self.start_ts > 0
            && now_ts > self.start_ts.saturating_add(self.duration)
    }

    /// State to render in the UI: `"expired"` when active+past-due,
    /// otherwise the raw on-chain state. The raw `state` field stays
    /// untouched so checks like `cmd_repay`'s `state == "active"`
    /// gate keep working.
    pub fn display_state_at(&self, now_ts: i64) -> &str {
        if self.is_expired_at(now_ts) {
            "expired"
        } else {
            self.state.as_str()
        }
    }
}

// ─── Client ──────────────────────────────────────────────────────────────────

pub struct SolanaClient {
    rpc: RpcClient,
    program_id: Pubkey,
}

impl SolanaClient {
    pub fn new(cfg: &Config) -> Result<Self> {
        let program_id = Pubkey::from_str(&cfg.program_id)
            .context("Invalid program ID")?;

        let rpc = RpcClient::new_with_commitment(
            cfg.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );

        Ok(Self { rpc, program_id })
    }

    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<f64> {
        let balance = self.rpc.get_balance(pubkey)?;
        Ok(balance as f64 / 1_000_000_000.0)
    }

    /// Returns true when the configured RPC points to Solana mainnet-beta.
    /// Compares the cluster's genesis hash against the canonical mainnet
    /// hash — works regardless of which RPC provider (Helius, QuickNode,
    /// Triton, vanity domains) the user is pointing at.
    pub async fn is_mainnet(&self) -> Result<bool> {
        let hash = self
            .rpc
            .get_genesis_hash()
            .context("Failed to fetch genesis hash")?;
        Ok(hash.to_string() == MAINNET_GENESIS_HASH)
    }

    // ─── PDA Derivations ─────────────────────────────────────────────────────

    fn config_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[CONFIG_SEED], &self.program_id)
    }

    fn vault_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[VAULT_SEED], &self.program_id)
    }

    fn admin_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[ADMIN_SEED], &self.program_id)
    }

    fn loan_pda(&self, loan_id: u64, borrower: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[LOAN_SEED, &loan_id.to_le_bytes(), borrower.as_ref()],
            &self.program_id,
        )
    }

    fn event_authority_pda(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[EVENT_AUTHORITY_SEED], &self.program_id)
    }

    // ─── Fetch On-Chain Data ─────────────────────────────────────────────────

    pub async fn fetch_protocol_config(&self) -> Result<ProtocolConfigInfo> {
        let (config_pda, _) = self.config_pda();
        let account = self
            .rpc
            .get_account(&config_pda)
            .context("Failed to fetch protocol config — is the program initialized?")?;

        let data = &account.data;

        // Parse fields from raw account data
        let admin = Pubkey::from(<[u8; 32]>::try_from(&data[8..40]).unwrap());
        let treasury = Pubkey::from(<[u8; 32]>::try_from(&data[40..72]).unwrap());
        let deployer = Pubkey::from(<[u8; 32]>::try_from(&data[72..104]).unwrap());
        let admin_fee_split_bps =
            u16::from_le_bytes(data[PROTOCOL_ADMIN_FEE_SPLIT_OFFSET..PROTOCOL_ADMIN_FEE_SPLIT_OFFSET + 2].try_into().unwrap());
        let default_interest_rate_bps =
            u16::from_le_bytes(data[PROTOCOL_DEFAULT_INTEREST_OFFSET..PROTOCOL_DEFAULT_INTEREST_OFFSET + 2].try_into().unwrap());
        let default_admin_fee_bps =
            u16::from_le_bytes(data[PROTOCOL_DEFAULT_ADMIN_FEE_OFFSET..PROTOCOL_DEFAULT_ADMIN_FEE_OFFSET + 2].try_into().unwrap());

        // 110: totalLoansOutstanding (u64), 118: totalShares (u64), 126: totalYieldDistributed (u64), 134: loanCounter (u64)
        let total_loans_outstanding =
            u64::from_le_bytes(data[110..118].try_into().unwrap());
        let total_shares =
            u64::from_le_bytes(data[118..126].try_into().unwrap());
        let total_yield_distributed =
            u64::from_le_bytes(data[126..134].try_into().unwrap());
        let loan_counter =
            u64::from_le_bytes(data[PROTOCOL_LOAN_COUNTER_OFFSET..PROTOCOL_LOAN_COUNTER_OFFSET + 8].try_into().unwrap());
        let is_paused = data[PROTOCOL_IS_PAUSED_OFFSET] != 0;

        Ok(ProtocolConfigInfo {
            admin,
            treasury,
            deployer,
            admin_fee_split_bps,
            default_interest_rate_bps,
            default_admin_fee_bps,
            total_loans_outstanding,
            total_shares,
            total_yield_distributed,
            loan_counter,
            is_paused,
        })
    }

    pub async fn fetch_loan(&self, loan_id: u64, borrower: &Pubkey) -> Result<LoanInfo> {
        let (loan_pda, _) = self.loan_pda(loan_id, borrower);
        let account = self
            .rpc
            .get_account(&loan_pda)
            .context("Failed to fetch loan account — does this loan exist?")?;

        let data = &account.data;

        let lid = u64::from_le_bytes(data[LOAN_ID_OFFSET..LOAN_ID_OFFSET + 8].try_into().unwrap());
        let loan_borrower = Pubkey::from(<[u8; 32]>::try_from(&data[LOAN_BORROWER_OFFSET..LOAN_BORROWER_OFFSET + 32]).unwrap());
        let program_pubkey = Pubkey::from(<[u8; 32]>::try_from(&data[LOAN_PROGRAM_PUBKEY_OFFSET..LOAN_PROGRAM_PUBKEY_OFFSET + 32]).unwrap());
        let principal = u64::from_le_bytes(data[LOAN_PRINCIPAL_OFFSET..LOAN_PRINCIPAL_OFFSET + 8].try_into().unwrap());
        let duration = i64::from_le_bytes(data[LOAN_DURATION_OFFSET..LOAN_DURATION_OFFSET + 8].try_into().unwrap());
        let interest_rate_bps = u16::from_le_bytes(data[LOAN_INTEREST_RATE_OFFSET..LOAN_INTEREST_RATE_OFFSET + 2].try_into().unwrap());
        let admin_fee_bps = u16::from_le_bytes(data[LOAN_ADMIN_FEE_BPS_OFFSET..LOAN_ADMIN_FEE_BPS_OFFSET + 2].try_into().unwrap());
        let start_ts = i64::from_le_bytes(data[LOAN_START_TS_OFFSET..LOAN_START_TS_OFFSET + 8].try_into().unwrap());

        let state_byte = data[LOAN_STATE_OFFSET];
        let state = match state_byte {
            0 => "active",
            1 => "repaid",
            2 => "recovered",
            3 => "pending",
            4 => "repaidPendingTransfer",
            5 => "reclaimed",
            _ => "unknown",
        }
        .to_string();

        // Calculate interest: principal * interestRateBps / 10000
        let interest_amount = principal
            .checked_mul(interest_rate_bps as u64)
            .unwrap_or(0)
            / 10_000;
        let total_repayment = principal.checked_add(interest_amount).unwrap_or(principal);

        Ok(LoanInfo {
            loan_id: lid,
            borrower: loan_borrower,
            program_pubkey,
            principal,
            duration,
            interest_rate_bps,
            admin_fee_bps,
            start_ts,
            state,
            interest_amount,
            total_repayment,
        })
    }

    /// Fetch the chain's Unix timestamp from the Clock sysvar — this is
    /// the same time source that on-chain programs see via
    /// `Clock::unix_timestamp`, and it's what `Loan.startTs` was stamped
    /// against. On localnet this can lag wall-clock by minutes, so any
    /// expiry comparison against `startTs` must use this value, not
    /// `chrono::Utc::now()`.
    pub async fn fetch_chain_timestamp(&self) -> Result<i64> {
        use solana_sdk::sysvar::clock;
        let account = self
            .rpc
            .get_account(&clock::ID)
            .context("Failed to fetch Clock sysvar")?;
        // Clock layout (Borsh, no discriminator): slot(u64) +
        // epochStartTimestamp(i64) + epoch(u64) + leaderScheduleEpoch(u64)
        // + unixTimestamp(i64). unixTimestamp lives at offset 32.
        let unix_ts = i64::from_le_bytes(
            account.data[32..40]
                .try_into()
                .context("Clock sysvar account shorter than expected")?,
        );
        Ok(unix_ts)
    }

    // ─── Transactions ────────────────────────────────────────────────────────

    /// Request a loan on-chain. Returns (signature, loan_id).
    pub async fn request_loan(
        &self,
        wallet: &Keypair,
        principal: u64,
        duration: i64,
        interest_rate_bps: u16,
        admin_fee_bps: u16,
    ) -> Result<(String, u64)> {
        // First get the current loan counter to derive the loan PDA
        let protocol_cfg = self.fetch_protocol_config().await?;
        let loan_id = protocol_cfg.loan_counter;

        let (config_pda, _) = self.config_pda();
        let (vault_pda, _) = self.vault_pda();
        let (admin_pda, _) = self.admin_pda();
        let (loan_pda, _) = self.loan_pda(loan_id, &wallet.pubkey());
        let (event_authority, _) = self.event_authority_pda();

        // Get deployer pubkey from protocol config
        let deployer_pubkey = protocol_cfg.deployer;

        // Build instruction data: discriminator + principal(u64) + duration(i64) + interestRateBps(u16) + adminFeeBps(u16)
        let mut ix_data = Vec::with_capacity(8 + 8 + 8 + 2 + 2);
        ix_data.extend_from_slice(&REQUEST_LOAN_DISC);
        ix_data.extend_from_slice(&principal.to_le_bytes());
        ix_data.extend_from_slice(&duration.to_le_bytes());
        ix_data.extend_from_slice(&interest_rate_bps.to_le_bytes());
        ix_data.extend_from_slice(&admin_fee_bps.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(wallet.pubkey(), true),      // borrower (writable, signer)
            AccountMeta::new(loan_pda, false),             // loan (writable)
            AccountMeta::new(config_pda, false),           // protocolConfig (writable)
            AccountMeta::new(vault_pda, false),            // vault (writable)
            AccountMeta::new(admin_pda, false),            // adminPda (writable)
            AccountMeta::new(deployer_pubkey, false),      // deployer (writable)
            AccountMeta::new_readonly(system_program::ID, false), // systemProgram
            AccountMeta::new_readonly(event_authority, false), // eventAuthority
            AccountMeta::new_readonly(self.program_id, false), // program
        ];

        let instruction = Instruction {
            program_id: self.program_id,
            accounts,
            data: ix_data,
        };

        let recent_blockhash = self.rpc.get_latest_blockhash()?;
        let tx = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&wallet.pubkey()),
            &[wallet],
            recent_blockhash,
        );

        let signature = self
            .rpc
            .send_and_confirm_transaction(&tx)
            .context("Failed to send loan request transaction")?;

        Ok((signature.to_string(), loan_id))
    }

    /// Repay a loan on-chain. Returns the transaction signature.
    pub async fn repay_loan(&self, wallet: &Keypair, loan_id: u64) -> Result<String> {
        let (config_pda, _) = self.config_pda();
        let (vault_pda, _) = self.vault_pda();
        let (admin_pda, _) = self.admin_pda();
        let (loan_pda, _) = self.loan_pda(loan_id, &wallet.pubkey());
        let (event_authority, _) = self.event_authority_pda();

        // Build instruction data: discriminator + loanId(u64)
        let mut ix_data = Vec::with_capacity(8 + 8);
        ix_data.extend_from_slice(&REPAY_LOAN_DISC);
        ix_data.extend_from_slice(&loan_id.to_le_bytes());

        let accounts = vec![
            AccountMeta::new(wallet.pubkey(), true),       // borrower (writable, signer)
            AccountMeta::new(loan_pda, false),              // loan (writable)
            AccountMeta::new(config_pda, false),            // protocolConfig (writable)
            AccountMeta::new(vault_pda, false),             // vault (writable)
            AccountMeta::new(admin_pda, false),             // adminPda (writable)
            AccountMeta::new_readonly(system_program::ID, false), // systemProgram
            AccountMeta::new_readonly(event_authority, false), // eventAuthority
            AccountMeta::new_readonly(self.program_id, false), // program
        ];

        let instruction = Instruction {
            program_id: self.program_id,
            accounts,
            data: ix_data,
        };

        let recent_blockhash = self.rpc.get_latest_blockhash()?;
        let tx = Transaction::new_signed_with_payer(
            &[instruction],
            Some(&wallet.pubkey()),
            &[wallet],
            recent_blockhash,
        );

        let signature = self
            .rpc
            .send_and_confirm_transaction(&tx)
            .context("Failed to send repay transaction")?;

        Ok(signature.to_string())
    }
}
