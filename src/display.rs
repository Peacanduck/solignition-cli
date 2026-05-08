use crate::client::{DeploymentInfo, FileUploadInfo};
use crate::config::Config;
use crate::solana_ops::ProtocolConfigInfo;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

// ─── Spinners & Progress ─────────────────────────────────────────────────────

pub fn spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

pub fn upload_progress_bar(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("  {spinner:.cyan} [{bar:40.cyan/dim}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏ "),
    );
    // Simulate progress since we upload in one shot
    pb.set_position(total);
    pb
}

// ─── Formatting Helpers ──────────────────────────────────────────────────────

pub fn short_sig(sig: &str) -> String {
    if sig.len() > 16 {
        format!("{}...{}", &sig[..8], &sig[sig.len() - 8..])
    } else {
        sig.to_string()
    }
}

pub fn short_pubkey(pk: &str) -> String {
    if pk.len() > 12 {
        format!("{}..{}", &pk[..6], &pk[pk.len() - 4..])
    } else {
        pk.to_string()
    }
}

pub fn format_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        if mins > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}h", hours)
        }
    } else {
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        if hours > 0 {
            format!("{}d {}h", days, hours)
        } else {
            format!("{}d", days)
        }
    }
}

fn format_timestamp(ts: u64) -> String {
    if ts == 0 {
        return "—".into();
    }
    // ts is in milliseconds
    let secs = if ts > 1_000_000_000_000 { ts / 1000 } else { ts };
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "invalid".into())
}

fn status_colored(status: &str) -> colored::ColoredString {
    match status {
        "deployed" => status.green().bold(),
        "pending" | "deploying" => status.yellow(),
        "failed" => status.red().bold(),
        "recovered" | "recovering" => status.magenta(),
        "ready" => status.green(),
        "active" => status.green().bold(),
        "repaid" => status.cyan().bold(),
        "repaidPendingTransfer" => "repaid (pending transfer)".yellow(),
        "reclaimed" => status.red(),
        _ => status.normal(),
    }
}

// ─── Config Display ──────────────────────────────────────────────────────────

pub fn print_config(cfg: &Config) {
    println!("  API URL:    {}", cfg.api_url.cyan());
    println!("  RPC URL:    {}", cfg.rpc_url.cyan());
    println!(
        "  Keypair:    {}",
        cfg.keypair_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(default ~/.config/solana/id.json)".into())
            .dimmed()
    );
    println!("  Program ID: {}", cfg.program_id.dimmed());
    println!(
        "  Config at:  {}",
        Config::config_path().display().to_string().dimmed()
    );
}

// ─── Deployment Status ───────────────────────────────────────────────────────

pub fn print_deployment_status(d: &DeploymentInfo) {
    println!("{}", "📋 Deployment Status".bold().cyan());
    println!("  ─────────────────────────────────────────");
    println!("  Loan ID:      {}", d.loan_id.yellow().bold());
    println!("  Status:       {}", status_colored(&d.status));
    println!("  Borrower:     {}", short_pubkey(&d.borrower).dimmed());

    if let Some(pid) = &d.program_id {
        println!("  Program ID:   {}", pid.green());
    }

    if let Some(cost) = d.deployment_cost {
        println!("  Deploy Cost:  {:.4} SOL", cost);
    }

    if let Some(sig) = &d.deploy_tx_signature {
        println!("  Deploy TX:    {}", short_sig(sig).dimmed());
    }

    if let Some(sig) = &d.set_deployed_tx_signature {
        println!("  Set Prog TX:  {}", short_sig(sig).dimmed());
    }

    if let Some(sig) = &d.recovery_tx_signature {
        println!("  Recovery TX:  {}", short_sig(sig).dimmed());
    }

    if let Some(err) = &d.error {
        println!("  Error:        {}", err.red());
    }

    if let Some(open) = d.program_account_open {
        println!(
            "  Account Open: {}",
            if open { "yes".green() } else { "no".dimmed() }
        );
    }

    println!("  Created:      {}", format_timestamp(d.created_at).dimmed());
    println!("  Updated:      {}", format_timestamp(d.updated_at).dimmed());
    println!("  ─────────────────────────────────────────");
}

// ─── Tables ──────────────────────────────────────────────────────────────────

pub fn print_uploads_table(uploads: &[FileUploadInfo]) {
    println!("{}", "📁 Your Uploads".bold().cyan());
    println!();
    println!(
        "  {:<18} {:<24} {:>10} {:>12} {:<8}",
        "FILE ID".bold(),
        "FILENAME".bold(),
        "SIZE".bold(),
        "COST (SOL)".bold(),
        "STATUS".bold()
    );
    println!("  {}", "─".repeat(76));

    for u in uploads {
        let size_str = if u.file_size > 1_048_576 {
            format!("{:.1} MB", u.file_size as f64 / 1_048_576.0)
        } else {
            format!("{:.0} KB", u.file_size as f64 / 1024.0)
        };

        println!(
            "  {:<18} {:<24} {:>10} {:>12} {}",
            &u.file_id[..16.min(u.file_id.len())],
            truncate_str(&u.file_name, 22),
            size_str,
            format!("{:.4}", u.estimated_cost),
            status_colored(&u.status),
        );
    }
    println!();
}

pub fn print_loans_table(deployments: &[DeploymentInfo]) {
    println!("{}", "🔧 Your Deployments".bold().cyan());
    println!();
    println!(
        "  {:<8} {:<12} {:<46} {:<12}",
        "LOAN ID".bold(),
        "STATUS".bold(),
        "PROGRAM ID".bold(),
        "UPDATED".bold(),
    );
    println!("  {}", "─".repeat(80));

    for d in deployments {
        let pid = d
            .program_id
            .as_deref()
            .unwrap_or("—");

        println!(
            "  {:<8} {:<22} {:<46} {:<12}",
            d.loan_id,
            status_colored(&d.status),
            if pid == "—" { pid.dimmed().to_string() } else { pid.to_string() },
            format_timestamp(d.updated_at),
        );
    }
    println!();
}

// ─── Protocol Config ─────────────────────────────────────────────────────────

pub fn print_protocol_config(cfg: &ProtocolConfigInfo) {
    println!("{}", "🏦 Protocol Configuration".bold().cyan());
    println!("  ─────────────────────────────────────────");
    println!("  Admin:             {}", cfg.admin.to_string().dimmed());
    println!("  Treasury:          {}", cfg.treasury.to_string().dimmed());
    println!("  Deployer:          {}", cfg.deployer.to_string().dimmed());
    println!(
        "  Admin Fee Split:   {} bps ({:.2}%)",
        cfg.admin_fee_split_bps,
        cfg.admin_fee_split_bps as f64 / 100.0
    );
    println!(
        "  Default Interest:  {} bps ({:.2}%)",
        cfg.default_interest_rate_bps,
        cfg.default_interest_rate_bps as f64 / 100.0
    );
    println!(
        "  Default Admin Fee: {} bps ({:.2}%)",
        cfg.default_admin_fee_bps,
        cfg.default_admin_fee_bps as f64 / 100.0
    );
    println!("  Total Loans Out:   {}", cfg.total_loans_outstanding);
    println!("  Total Shares:      {}", cfg.total_shares);
    println!(
        "  Yield Distributed: {:.4} SOL",
        cfg.total_yield_distributed as f64 / 1_000_000_000.0
    );
    println!("  Loan Counter:      {}", cfg.loan_counter);
    println!(
        "  Paused:            {}",
        if cfg.is_paused {
            "YES".red().bold()
        } else {
            "no".green()
        }
    );
    println!("  ─────────────────────────────────────────");
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}
