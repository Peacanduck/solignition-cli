use anyhow::{Context, Result};
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ─── Response Types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UploadResponse {
    pub success: bool,
    #[serde(rename = "fileId")]
    pub file_id: String,
    #[serde(rename = "estimatedCost")]
    pub estimated_cost: f64,
    #[serde(rename = "binaryHash")]
    pub binary_hash: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct FileUploadInfo {
    #[serde(rename = "fileId")]
    pub file_id: String,
    pub borrower: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "fileSize")]
    pub file_size: u64,
    #[serde(rename = "binaryHash")]
    pub binary_hash: String,
    #[serde(rename = "estimatedCost")]
    pub estimated_cost: f64,
    pub status: String,
    #[serde(rename = "createdAt")]
    pub created_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct NotifyLoanResponse {
    pub success: bool,
    pub message: String,
    pub signature: Option<String>,
    #[serde(rename = "fileId")]
    pub file_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NotifyRepaidResponse {
    pub success: bool,
    pub message: String,
    pub tx: Option<String>,
    #[serde(rename = "loanId")]
    pub loan_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DeploymentInfo {
    #[serde(rename = "loanId")]
    pub loan_id: String,
    pub borrower: String,
    #[serde(rename = "programId")]
    pub program_id: Option<String>,
    #[serde(rename = "deploymentCost")]
    pub deployment_cost: Option<f64>,
    #[serde(rename = "deployTxSignature")]
    pub deploy_tx_signature: Option<String>,
    #[serde(rename = "setDeployedTxSignature")]
    pub set_deployed_tx_signature: Option<String>,
    #[serde(rename = "recoveryTxSignature")]
    pub recovery_tx_signature: Option<String>,
    pub status: String,
    pub error: Option<String>,
    #[serde(rename = "createdAt")]
    pub created_at: u64,
    #[serde(rename = "updatedAt")]
    pub updated_at: u64,
    #[serde(rename = "binaryHash")]
    pub binary_hash: Option<String>,
    pub principal: Option<String>,
    #[serde(rename = "programAccountOpen")]
    pub program_account_open: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    #[serde(rename = "activeLoans")]
    pub active_loans: u64,
    #[serde(rename = "totalDeployments")]
    pub total_deployments: u64,
    pub timestamp: String,
}

#[derive(Debug, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ─── Client ──────────────────────────────────────────────────────────────────

pub struct DeployerClient {
    client: reqwest::Client,
    base_url: String,
}

impl DeployerClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Upload a .so binary file
    pub async fn upload_file(&self, file_path: &Path, borrower: &str) -> Result<UploadResponse> {
        let file_name = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let file_bytes = tokio::fs::read(file_path).await
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        let file_part = multipart::Part::bytes(file_bytes)
            .file_name(file_name)
            .mime_str("application/octet-stream")?;

        let form = multipart::Form::new()
            .text("borrower", borrower.to_string())
            .part("file", file_part);

        let resp = self
            .client
            .post(format!("{}/upload", self.base_url))
            .multipart(form)
            .send()
            .await
            .context("Failed to connect to deployer API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Upload failed ({}): {}", status, body);
        }

        resp.json::<UploadResponse>()
            .await
            .context("Failed to parse upload response")
    }

    /// Get upload info by file ID
    pub async fn get_upload(&self, file_id: &str) -> Result<FileUploadInfo> {
        let resp = self
            .client
            .get(format!("{}/uploads/{}", self.base_url, file_id))
            .send()
            .await
            .context("Failed to connect to deployer API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get upload ({}): {}", status, body);
        }

        resp.json::<FileUploadInfo>()
            .await
            .context("Failed to parse upload info")
    }

    /// Get all uploads for a borrower
    pub async fn get_uploads_by_borrower(&self, borrower: &str) -> Result<Vec<FileUploadInfo>> {
        let resp = self
            .client
            .get(format!("{}/uploads/borrower/{}", self.base_url, borrower))
            .send()
            .await
            .context("Failed to connect to deployer API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get uploads ({}): {}", status, body);
        }

        resp.json::<Vec<FileUploadInfo>>()
            .await
            .context("Failed to parse uploads")
    }

    /// Notify deployer about a new loan request
    pub async fn notify_loan(
        &self,
        signature: &str,
        borrower: &str,
        loan_id: &str,
        file_id: &str,
    ) -> Result<NotifyLoanResponse> {
        let body = serde_json::json!({
            "signature": signature,
            "borrower": borrower,
            "loanId": loan_id,
            "fileId": file_id,
        });

        let resp = self
            .client
            .post(format!("{}/notify-loan", self.base_url))
            .json(&body)
            .send()
            .await
            .context("Failed to connect to deployer API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to notify loan ({}): {}", status, body);
        }

        resp.json::<NotifyLoanResponse>()
            .await
            .context("Failed to parse notify response")
    }

    /// Notify deployer that a loan has been repaid
    pub async fn notify_repaid(
        &self,
        signature: &str,
        borrower: &str,
        loan_id: u64,
    ) -> Result<NotifyRepaidResponse> {
        let body = serde_json::json!({
            "signature": signature,
            "borrower": borrower,
            "loanId": loan_id.to_string(),
        });

        let resp = self
            .client
            .post(format!("{}/notify-repaid", self.base_url))
            .json(&body)
            .send()
            .await
            .context("Failed to connect to deployer API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to notify repaid ({}): {}", status, body);
        }

        resp.json::<NotifyRepaidResponse>()
            .await
            .context("Failed to parse repaid response")
    }

    /// Get deployment status
    pub async fn get_deployment(&self, loan_id: &str) -> Result<DeploymentInfo> {
        let resp = self
            .client
            .get(format!("{}/deployments/{}", self.base_url, loan_id))
            .send()
            .await
            .context("Failed to connect to deployer API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get deployment ({}): {}", status, body);
        }

        resp.json::<DeploymentInfo>()
            .await
            .context("Failed to parse deployment info")
    }

    /// Get all deployments for a borrower
    pub async fn get_deployments_by_borrower(
        &self,
        borrower: &str,
    ) -> Result<Vec<DeploymentInfo>> {
        let resp = self
            .client
            .get(format!(
                "{}/deployments/borrower/{}",
                self.base_url, borrower
            ))
            .send()
            .await
            .context("Failed to connect to deployer API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Failed to get deployments ({}): {}", status, body);
        }

        resp.json::<Vec<DeploymentInfo>>()
            .await
            .context("Failed to parse deployments")
    }

    /// Health check
    pub async fn health(&self) -> Result<HealthResponse> {
        let resp = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .context("Failed to connect to deployer API — is the service running?")?;

        resp.json::<HealthResponse>()
            .await
            .context("Failed to parse health response")
    }
}
