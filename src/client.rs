// Response structs intentionally expose every field returned by the deployer
// service for forward compatibility and Debug output, even when not all
// fields are read by the CLI today.
#![allow(dead_code)]

use anyhow::{anyhow, Context, Result};
use rand::RngCore;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::multipart;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Wire-format version tag for the shared CLI/frontend auth spec.
const AUTH_VERSION_TAG: &str = "solignition-auth-v1";

/// SHA-256 of the empty string, in lowercase hex. Used as `BODY_HASH_HEX` for
/// GET requests and any other body-less request.
const EMPTY_BODY_HASH: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// Auth headers produced by signing the canonical request message.
struct AuthHeaders {
    pubkey_b58: String,
    timestamp_ms: String,
    nonce_b58: String,
    signature_b58: String,
}

impl AuthHeaders {
    fn apply(self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req = req.header("X-Auth-Pubkey", self.pubkey_b58);
        req = req.header("X-Auth-Timestamp", self.timestamp_ms);
        req = req.header("X-Auth-Nonce", self.nonce_b58);
        req = req.header("X-Auth-Signature", self.signature_b58);
        req
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex::encode(digest)
}

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
    /// Signer used to produce `X-Auth-*` headers. `None` only for the anonymous
    /// `/health` call site (constructed via `new_anonymous`).
    signer: Option<Arc<Keypair>>,
}

impl DeployerClient {
    fn build_client() -> reqwest::Client {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(
            "ngrok-skip-browser-warning",
            HeaderValue::from_static("true"),
        );

        reqwest::Client::builder()
            .user_agent(concat!("solignition-cli/", env!("CARGO_PKG_VERSION")))
            .default_headers(default_headers)
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client")
    }

    pub fn new(base_url: &str, signer: Arc<Keypair>) -> Self {
        Self {
            client: Self::build_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
            signer: Some(signer),
        }
    }

    /// Anonymous client for endpoints that don't require auth (e.g. `/health`).
    pub fn new_anonymous(base_url: &str) -> Self {
        Self {
            client: Self::build_client(),
            base_url: base_url.trim_end_matches('/').to_string(),
            signer: None,
        }
    }

    fn signer(&self) -> Result<&Keypair> {
        self.signer
            .as_deref()
            .ok_or_else(|| anyhow!("DeployerClient was constructed anonymously; cannot sign request"))
    }

    /// Build the `X-Auth-*` headers for a request per the shared auth spec.
    ///
    /// `body_hash_hex` must be the lowercase-hex SHA-256 of the exact body bytes
    /// the client will write to the wire (file bytes for multipart, raw JSON bytes
    /// for JSON, `EMPTY_BODY_HASH` for body-less requests).
    fn sign_request(&self, method: &str, path: &str, body_hash_hex: &str) -> Result<AuthHeaders> {
        let signer = self.signer()?;

        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis().to_string())
            .unwrap_or_else(|_| "0".to_string());

        let mut nonce_bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce_b58 = bs58::encode(nonce_bytes).into_string();

        let canonical = format!(
            "{tag}\n{method}\n{path}\n{ts}\n{nonce}\n{body_hash}",
            tag = AUTH_VERSION_TAG,
            method = method,
            path = path,
            ts = timestamp_ms,
            nonce = nonce_b58,
            body_hash = body_hash_hex,
        );

        let signature = signer.sign_message(canonical.as_bytes());
        let signature_b58 = bs58::encode(signature.as_ref()).into_string();

        Ok(AuthHeaders {
            pubkey_b58: signer.pubkey().to_string(),
            timestamp_ms,
            nonce_b58,
            signature_b58,
        })
    }

    /// Upload a .so binary file.
    ///
    /// Signs the request with the borrower's keypair and verifies the deployer's
    /// returned `binary_hash` matches the local SHA-256 — guards against silent
    /// transit corruption or a substituted binary.
    pub async fn upload_file(&self, file_path: &Path, borrower: &str) -> Result<UploadResponse> {
        let file_name = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let file_bytes = tokio::fs::read(file_path).await
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        let local_hash = sha256_hex(&file_bytes);

        // For multipart `/upload`, the body hash is sha256(file_bytes) only —
        // the `borrower` form field is enforced separately via authz.
        let auth = self.sign_request("POST", "/upload", &local_hash)?;

        let file_part = multipart::Part::bytes(file_bytes)
            .file_name(file_name)
            .mime_str("application/octet-stream")?;

        let form = multipart::Form::new()
            .text("borrower", borrower.to_string())
            .part("file", file_part);

        let req = self
            .client
            .post(format!("{}/upload", self.base_url))
            .multipart(form);

        let resp = auth
            .apply(req)
            .send()
            .await
            .context("Failed to connect to deployer API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Upload failed ({}): {}", status, body);
        }

        let parsed: UploadResponse = resp
            .json()
            .await
            .context("Failed to parse upload response")?;

        if !parsed.binary_hash.eq_ignore_ascii_case(&local_hash) {
            anyhow::bail!(
                "Upload integrity check failed: deployer reported binary_hash `{}` but local file hash is `{}`",
                parsed.binary_hash,
                local_hash,
            );
        }

        Ok(parsed)
    }

    /// Get upload info by file ID
    pub async fn get_upload(&self, file_id: &str) -> Result<FileUploadInfo> {
        let path = format!("/uploads/{}", file_id);
        let auth = self.sign_request("GET", &path, EMPTY_BODY_HASH)?;

        let req = self.client.get(format!("{}{}", self.base_url, path));
        let resp = auth
            .apply(req)
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
        let path = format!("/uploads/borrower/{}", borrower);
        let auth = self.sign_request("GET", &path, EMPTY_BODY_HASH)?;

        let req = self.client.get(format!("{}{}", self.base_url, path));
        let resp = auth
            .apply(req)
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

        // Serialize once and sign over the exact bytes we'll write to the wire.
        // Using `.json(&body)` would let reqwest re-serialize and produce
        // potentially different bytes than what we hashed.
        let body_bytes = serde_json::to_vec(&body)
            .context("Failed to serialize notify-loan body")?;
        let body_hash = sha256_hex(&body_bytes);
        let auth = self.sign_request("POST", "/notify-loan", &body_hash)?;

        let req = self
            .client
            .post(format!("{}/notify-loan", self.base_url))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body_bytes);

        let resp = auth
            .apply(req)
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

        let body_bytes = serde_json::to_vec(&body)
            .context("Failed to serialize notify-repaid body")?;
        let body_hash = sha256_hex(&body_bytes);
        let auth = self.sign_request("POST", "/notify-repaid", &body_hash)?;

        let req = self
            .client
            .post(format!("{}/notify-repaid", self.base_url))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body_bytes);

        let resp = auth
            .apply(req)
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
        let path = format!("/deployments/{}", loan_id);
        let auth = self.sign_request("GET", &path, EMPTY_BODY_HASH)?;

        let req = self.client.get(format!("{}{}", self.base_url, path));
        let resp = auth
            .apply(req)
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
        let path = format!("/deployments/borrower/{}", borrower);
        let auth = self.sign_request("GET", &path, EMPTY_BODY_HASH)?;

        let req = self.client.get(format!("{}{}", self.base_url, path));
        let resp = auth
            .apply(req)
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
