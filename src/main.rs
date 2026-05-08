mod client;
mod config;
mod display;
mod solana_ops;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "solignition",
    about = "🔥 Solignition CLI — Deploy Solana programs without upfront capital",
    version,
    long_about = "Deploy your Solana programs using the Solignition lending protocol.\n\
                   Upload your .so binary, request a deployment loan, and get your program live on-chain."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Deployer API URL
    #[arg(long, env = "SOLIGNITION_API_URL", global = true)]
    api_url: Option<String>,

    /// Solana RPC URL
    #[arg(long, env = "SOLANA_RPC_URL", global = true)]
    rpc_url: Option<String>,

    /// Path to wallet keypair JSON file
    #[arg(short, long, env = "SOLIGNITION_KEYPAIR", global = true)]
    keypair: Option<PathBuf>,

    /// Solignition program ID
    #[arg(long, env = "SOLIGNITION_PROGRAM_ID", global = true)]
    program_id: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize CLI configuration
    Init {
        /// Deployer API URL
        #[arg(long)]
        api_url: Option<String>,

        /// Solana RPC URL
        #[arg(long)]
        rpc_url: Option<String>,

        /// Path to wallet keypair
        #[arg(long)]
        keypair: Option<PathBuf>,

        /// Solignition program ID
        #[arg(long)]
        program_id: Option<String>,
    },

    /// Show current configuration
    Config,

    /// Upload a compiled Solana program (.so file)
    Upload {
        /// Path to the .so binary file
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },

    /// Request a deployment loan and deploy your program
    Deploy {
        /// File ID from a previous upload (skip upload step)
        #[arg(long)]
        file_id: Option<String>,

        /// Path to .so file (uploads and deploys in one step)
        #[arg(long)]
        file: Option<PathBuf>,

        /// Loan duration in seconds (default: 7 days)
        #[arg(long, default_value = "604800")]
        duration: i64,

        /// Custom interest rate in basis points
        #[arg(long)]
        interest_rate_bps: Option<u16>,

        /// Custom admin fee in basis points
        #[arg(long)]
        admin_fee_bps: Option<u16>,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Repay an active loan and claim program ownership
    Repay {
        /// Loan ID to repay
        loan_id: u64,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Check deployment status for a loan
    Status {
        /// Loan ID to check
        loan_id: u64,
    },

    /// List your uploads
    Uploads,

    /// List your deployments/loans
    Loans,

    /// Check protocol health and stats
    Health,

    /// Show your wallet info and balance
    Wallet,

    /// Fetch on-chain protocol config
    ProtocolInfo,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load or create config
    let mut cfg = config::Config::load().unwrap_or_default();

    // Apply CLI overrides
    if let Some(url) = &cli.api_url {
        cfg.api_url = url.clone();
    }
    if let Some(url) = &cli.rpc_url {
        cfg.rpc_url = url.clone();
    }
    if let Some(kp) = &cli.keypair {
        cfg.keypair_path = Some(kp.clone());
    }
    if let Some(pid) = &cli.program_id {
        cfg.program_id = pid.clone();
    }

    match cli.command {
        Commands::Init {
            api_url,
            rpc_url,
            keypair,
            program_id,
        } => cmd_init(api_url, rpc_url, keypair, program_id).await,

        Commands::Config => cmd_config(&cfg).await,

        Commands::Upload { file } => cmd_upload(&cfg, file).await,

        Commands::Deploy {
            file_id,
            file,
            duration,
            interest_rate_bps,
            admin_fee_bps,
            yes,
        } => {
            cmd_deploy(
                &cfg,
                file_id,
                file,
                duration,
                interest_rate_bps,
                admin_fee_bps,
                yes,
            )
            .await
        }

        Commands::Repay { loan_id, yes } => cmd_repay(&cfg, loan_id, yes).await,

        Commands::Status { loan_id } => cmd_status(&cfg, loan_id).await,

        Commands::Uploads => cmd_uploads(&cfg).await,

        Commands::Loans => cmd_loans(&cfg).await,

        Commands::Health => cmd_health(&cfg).await,

        Commands::Wallet => cmd_wallet(&cfg).await,

        Commands::ProtocolInfo => cmd_protocol_info(&cfg).await,
    }
}

// ─── Command Implementations ─────────────────────────────────────────────────

async fn cmd_init(
    api_url: Option<String>,
    rpc_url: Option<String>,
    keypair: Option<PathBuf>,
    program_id: Option<String>,
) -> Result<()> {
    println!("{}", "🔥 Solignition CLI Setup".bold().cyan());
    println!();

    let api = if let Some(url) = api_url {
        url
    } else {
        dialoguer::Input::new()
            .with_prompt("Deployer API URL")
            .default("http://localhost:3000".into())
            .interact_text()?
    };

    let rpc = if let Some(url) = rpc_url {
        url
    } else {
        dialoguer::Input::new()
            .with_prompt("Solana RPC URL")
            .default("http://127.0.0.1:8899".into())
            .interact_text()?
    };

    let kp = if let Some(path) = keypair {
        path
    } else {
        let default_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".config/solana/id.json");
        let input: String = dialoguer::Input::new()
            .with_prompt("Keypair path")
            .default(default_path.to_string_lossy().into_owned())
            .interact_text()?;
        PathBuf::from(input)
    };

    let pid = if let Some(id) = program_id {
        id
    } else {
        dialoguer::Input::new()
            .with_prompt("Solignition Program ID")
            .default("Dz4Zey62uraTxX9V9HBXpCfuFtNzdt5ULNQ1yZXh6Peh".into())
            .interact_text()?
    };

    let cfg = config::Config {
        api_url: api,
        rpc_url: rpc,
        keypair_path: Some(kp),
        program_id: pid,
    };

    cfg.save()?;
    println!();
    println!("{}", "✅ Configuration saved!".green().bold());
    display::print_config(&cfg);

    Ok(())
}

async fn cmd_config(cfg: &config::Config) -> Result<()> {
    println!("{}", "⚙️  Current Configuration".bold().cyan());
    println!();
    display::print_config(cfg);
    Ok(())
}

async fn cmd_upload(cfg: &config::Config, file: PathBuf) -> Result<()> {
    // Validate file exists and is .so
    if !file.exists() {
        anyhow::bail!("File not found: {}", file.display());
    }
    if file.extension().and_then(|e| e.to_str()) != Some("so") {
        anyhow::bail!("Only .so files are accepted");
    }

    let wallet = config::load_keypair(cfg)?;
    let borrower = wallet.pubkey().to_string();

    let metadata = std::fs::metadata(&file)?;
    println!("{}", "📦 Uploading program binary...".bold().cyan());
    println!(
        "  File:     {}",
        file.file_name().unwrap().to_string_lossy()
    );
    println!("  Size:     {} bytes ({:.2} MB)", metadata.len(), metadata.len() as f64 / 1_048_576.0);
    println!("  Borrower: {}", borrower.dimmed());
    println!();

    let api = client::DeployerClient::new(&cfg.api_url);
    let pb = display::upload_progress_bar(metadata.len());

    let resp = api.upload_file(&file, &borrower).await?;
    pb.finish_with_message("Upload complete");

    println!();
    println!("{}", "✅ Upload successful!".green().bold());
    println!("  File ID:        {}", resp.file_id.yellow().bold());
    println!(
        "  Estimated Cost: {} SOL",
        format!("{:.4}", resp.estimated_cost).cyan()
    );
    println!("  Binary Hash:    {}", resp.binary_hash.dimmed());
    println!();
    println!(
        "{}",
        "Next: Run `solignition deploy` to request a loan and deploy.".dimmed()
    );
    println!(
        "  {}",
        format!("solignition deploy --file-id {}", resp.file_id).dimmed()
    );

    Ok(())
}

async fn cmd_deploy(
    cfg: &config::Config,
    file_id: Option<String>,
    file: Option<PathBuf>,
    duration: i64,
    interest_rate_bps: Option<u16>,
    admin_fee_bps: Option<u16>,
    skip_confirm: bool,
) -> Result<()> {
    let wallet = config::load_keypair(cfg)?;
    let borrower = wallet.pubkey().to_string();
    let api = client::DeployerClient::new(&cfg.api_url);

    // Step 1: Ensure we have a file_id
    let fid = if let Some(id) = file_id {
        id
    } else if let Some(path) = file {
        println!("{}", "📦 Uploading program binary first...".bold().cyan());
        let resp = api.upload_file(&path, &borrower).await?;
        println!(
            "  ✅ Uploaded — File ID: {} | Cost: {:.4} SOL",
            resp.file_id.yellow(),
            resp.estimated_cost
        );
        println!();
        resp.file_id
    } else {
        anyhow::bail!(
            "Provide either --file-id <ID> or --file <path.so>\n\
             Run `solignition uploads` to see your uploaded files."
        );
    };

    // Step 2: Get upload info for cost estimate
    let upload_info = api
        .get_upload(&fid)
        .await
        .context("Failed to fetch upload info. Is the file ID correct?")?;

    let principal_sol = upload_info.estimated_cost;
    let principal_lamports = (principal_sol * 1_000_000_000.0) as u64;

    // Step 3: Fetch protocol defaults
    let sol_client = solana_ops::SolanaClient::new(cfg)?;
    let protocol_cfg = sol_client.fetch_protocol_config().await?;

    let interest = interest_rate_bps.unwrap_or(protocol_cfg.default_interest_rate_bps);
    let admin_fee = admin_fee_bps.unwrap_or(protocol_cfg.default_admin_fee_bps);

    // Step 4: Show summary and confirm
    println!("{}", "🚀 Deployment Summary".bold().cyan());
    println!("  ─────────────────────────────────────");
    println!("  File ID:       {}", fid.yellow());
    println!("  Binary:        {}", upload_info.file_name);
    println!(
        "  Principal:     {} SOL",
        format!("{:.4}", principal_sol).cyan().bold()
    );
    println!(
        "  Duration:      {} ({})",
        display::format_duration(duration),
        format!("{}s", duration).dimmed()
    );
    println!("  Interest Rate: {} bps ({:.2}%)", interest, interest as f64 / 100.0);
    println!("  Admin Fee:     {} bps ({:.2}%)", admin_fee, admin_fee as f64 / 100.0);
    println!("  Borrower:      {}", borrower.dimmed());
    println!("  ─────────────────────────────────────");

    // Calculate total repayment
    let interest_amount = principal_sol * (interest as f64 / 10_000.0);
    let total_repay = principal_sol + interest_amount;
    println!(
        "  Total Repay:   ~{} SOL",
        format!("{:.4}", total_repay).yellow()
    );
    println!();

    if !skip_confirm {
        let confirmed = dialoguer::Confirm::new()
            .with_prompt("Proceed with deployment?")
            .default(true)
            .interact()?;

        if !confirmed {
            println!("{}", "Cancelled.".dimmed());
            return Ok(());
        }
    }

    // Step 5: Send on-chain loan request transaction
    println!();
    let spinner = display::spinner("Submitting loan request on-chain...");

    let (signature, loan_id) = sol_client
        .request_loan(&wallet, principal_lamports, duration, interest, admin_fee)
        .await
        .context("Failed to submit loan request transaction")?;

    spinner.finish_with_message(format!(
        "✅ Loan requested — TX: {}",
        display::short_sig(&signature)
    ));

    println!("  Loan ID:    {}", loan_id.to_string().yellow().bold());
    println!("  Signature:  {}", signature.dimmed());
    println!();

    // Step 6: Notify deployer to begin deployment
    let spinner = display::spinner("Notifying deployer service...");

    api.notify_loan(&signature, &borrower, &loan_id.to_string(), &fid)
        .await
        .context("Failed to notify deployer")?;

    spinner.finish_with_message("✅ Deployer notified — deployment in progress");
    println!();
    println!(
        "{}",
        "Your program is being deployed! Check status with:".dimmed()
    );
    println!("  {}", format!("solignition status {}", loan_id).dimmed());

    Ok(())
}

async fn cmd_repay(cfg: &config::Config, loan_id: u64, skip_confirm: bool) -> Result<()> {
    let wallet = config::load_keypair(cfg)?;
    let sol_client = solana_ops::SolanaClient::new(cfg)?;

    // Fetch loan info
    let spinner = display::spinner("Fetching loan details...");
    let loan = sol_client.fetch_loan(loan_id, &wallet.pubkey()).await?;
    spinner.finish_and_clear();

    let principal_sol = loan.principal as f64 / 1_000_000_000.0;
    let interest_sol = loan.interest_amount as f64 / 1_000_000_000.0;
    let total_sol = loan.total_repayment as f64 / 1_000_000_000.0;

    println!("{}", "💰 Loan Repayment".bold().cyan());
    println!("  ─────────────────────────────────────");
    println!("  Loan ID:     {}", loan_id.to_string().yellow());
    println!("  Principal:   {:.4} SOL", principal_sol);
    println!("  Interest:    {:.4} SOL", interest_sol);
    println!(
        "  Total Due:   {} SOL",
        format!("{:.4}", total_sol).cyan().bold()
    );
    println!("  State:       {}", loan.state);
    println!("  ─────────────────────────────────────");
    println!();

    if loan.state != "active" {
        anyhow::bail!("Loan is not in active state (current: {})", loan.state);
    }

    if !skip_confirm {
        let confirmed = dialoguer::Confirm::new()
            .with_prompt(format!("Repay {:.4} SOL?", total_sol))
            .default(true)
            .interact()?;
        if !confirmed {
            println!("{}", "Cancelled.".dimmed());
            return Ok(());
        }
    }

    let spinner = display::spinner("Submitting repayment transaction...");
    let signature = sol_client.repay_loan(&wallet, loan_id).await?;
    spinner.finish_with_message(format!(
        "✅ Repayment submitted — TX: {}",
        display::short_sig(&signature)
    ));

    println!();

    // Notify deployer to transfer authority
    let spinner = display::spinner("Requesting program authority transfer...");
    let api = client::DeployerClient::new(&cfg.api_url);
    api.notify_repaid(&signature, &wallet.pubkey().to_string(), loan_id)
        .await?;
    spinner.finish_with_message("✅ Authority transfer initiated");

    println!();
    println!(
        "{}",
        "🎉 Loan repaid! Program authority will be transferred to your wallet.".green().bold()
    );
    println!(
        "{}",
        "   This may take a few moments. Check status with:".dimmed()
    );
    println!("   {}", format!("solignition status {}", loan_id).dimmed());

    Ok(())
}

async fn cmd_status(cfg: &config::Config, loan_id: u64) -> Result<()> {
    let api = client::DeployerClient::new(&cfg.api_url);

    let spinner = display::spinner("Fetching deployment status...");
    let deployment = api.get_deployment(&loan_id.to_string()).await?;
    spinner.finish_and_clear();

    display::print_deployment_status(&deployment);

    Ok(())
}

async fn cmd_uploads(cfg: &config::Config) -> Result<()> {
    let wallet = config::load_keypair(cfg)?;
    let api = client::DeployerClient::new(&cfg.api_url);

    let spinner = display::spinner("Fetching uploads...");
    let uploads = api.get_uploads_by_borrower(&wallet.pubkey().to_string()).await?;
    spinner.finish_and_clear();

    if uploads.is_empty() {
        println!("{}", "No uploads found.".dimmed());
        println!(
            "{}",
            "Upload a program with: solignition upload <file.so>".dimmed()
        );
        return Ok(());
    }

    display::print_uploads_table(&uploads);
    Ok(())
}

async fn cmd_loans(cfg: &config::Config) -> Result<()> {
    let wallet = config::load_keypair(cfg)?;
    let api = client::DeployerClient::new(&cfg.api_url);

    let spinner = display::spinner("Fetching deployments...");
    let deployments = api
        .get_deployments_by_borrower(&wallet.pubkey().to_string())
        .await?;
    spinner.finish_and_clear();

    if deployments.is_empty() {
        println!("{}", "No deployments found.".dimmed());
        return Ok(());
    }

    display::print_loans_table(&deployments);
    Ok(())
}

async fn cmd_health(cfg: &config::Config) -> Result<()> {
    let api = client::DeployerClient::new(&cfg.api_url);

    let spinner = display::spinner("Checking deployer health...");
    let health = api.health().await?;
    spinner.finish_and_clear();

    println!("{}", "🏥 Deployer Health".bold().cyan());
    println!("  Status:      {}", if health.status == "healthy" {
        health.status.green().bold()
    } else {
        health.status.red().bold()
    });
    println!("  Active Loans:      {}", health.active_loans);
    println!("  Total Deployments: {}", health.total_deployments);
    println!("  Timestamp:         {}", health.timestamp.dimmed());
    Ok(())
}

async fn cmd_wallet(cfg: &config::Config) -> Result<()> {
    let wallet = config::load_keypair(cfg)?;
    let sol_client = solana_ops::SolanaClient::new(cfg)?;

    let pubkey = wallet.pubkey();
    let balance = sol_client.get_balance(&pubkey).await?;

    println!("{}", "👛 Wallet Info".bold().cyan());
    println!("  Address: {}", pubkey.to_string().yellow());
    println!(
        "  Balance: {} SOL",
        format!("{:.4}", balance).cyan().bold()
    );
    Ok(())
}

async fn cmd_protocol_info(cfg: &config::Config) -> Result<()> {
    let sol_client = solana_ops::SolanaClient::new(cfg)?;

    let spinner = display::spinner("Fetching protocol config...");
    let protocol_cfg = sol_client.fetch_protocol_config().await?;
    spinner.finish_and_clear();

    display::print_protocol_config(&protocol_cfg);
    Ok(())
}
