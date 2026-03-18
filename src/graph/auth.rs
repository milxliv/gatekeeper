// Microsoft Graph API - Client Credentials OAuth2 token flow
// No user login required. Uses app registration (client_id + client_secret + tenant_id).

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::Deserialize;
use anyhow::{Context, Result};

/// Token response from Microsoft identity platform
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    #[allow(dead_code)]
    token_type: String,
}

/// Cached token with expiry tracking
#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

impl CachedToken {
    /// Returns true if token is still valid (with 60s buffer)
    fn is_valid(&self) -> bool {
        Instant::now() + Duration::from_secs(60) < self.expires_at
    }
}

/// Configuration loaded from environment variables
#[derive(Debug, Clone)]
pub struct GraphConfig {
    pub tenant_id: String,
    pub client_id: String,
    pub client_secret: String,
    pub group_id: String,
    pub group_email: String,
    pub receptionist_email: Option<String>,
}

impl GraphConfig {
    /// Load from environment variables
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            tenant_id:     std::env::var("GRAPH_TENANT_ID")
                .context("GRAPH_TENANT_ID not set")?,
            client_id:     std::env::var("GRAPH_CLIENT_ID")
                .context("GRAPH_CLIENT_ID not set")?,
            client_secret: std::env::var("GRAPH_CLIENT_SECRET")
                .context("GRAPH_CLIENT_SECRET not set")?,
            group_id:      std::env::var("GRAPH_GROUP_ID")
                .context("GRAPH_GROUP_ID not set")?,
            group_email:   std::env::var("GRAPH_GROUP_EMAIL")
                .context("GRAPH_GROUP_EMAIL not set")?,
            receptionist_email: std::env::var("RECEPTIONIST_EMAIL").ok()
                .filter(|s| !s.trim().is_empty()),
        })
    }
}

/// Thread-safe token provider with automatic refresh
#[derive(Debug, Clone)]
pub struct TokenProvider {
    config: Arc<GraphConfig>,
    cached: Arc<RwLock<Option<CachedToken>>>,
    client: reqwest::Client,
}

impl TokenProvider {
    pub fn new(config: GraphConfig, client: reqwest::Client) -> Self {
        Self {
            config: Arc::new(config),
            cached: Arc::new(RwLock::new(None)),
            client,
        }
    }

    /// Returns a valid Bearer token, refreshing if needed
    pub async fn get_token(&self) -> Result<String> {
        // Fast path: read lock, check validity
        {
            let lock = self.cached.read().await;
            if let Some(ref t) = *lock {
                if t.is_valid() {
                    return Ok(t.access_token.clone());
                }
            }
        }

        // Slow path: write lock, fetch new token
        let mut lock = self.cached.write().await;

        // Double-check after acquiring write lock
        if let Some(ref t) = *lock {
            if t.is_valid() {
                return Ok(t.access_token.clone());
            }
        }

        let token = self.fetch_token().await?;
        *lock = Some(token.clone());
        Ok(token.access_token)
    }

    async fn fetch_token(&self) -> Result<CachedToken> {
        let url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.config.tenant_id
        );

        let params = [
            ("grant_type",    "client_credentials"),
            ("client_id",     &self.config.client_id),
            ("client_secret", &self.config.client_secret),
            ("scope",         "https://graph.microsoft.com/.default"),
        ];

        let resp = self.client
            .post(&url)
            .form(&params)
            .send()
            .await
            .context("Token request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Token error {status}: {body}");
        }

        let token_resp: TokenResponse = resp.json().await
            .context("Failed to parse token response")?;

        Ok(CachedToken {
            access_token: token_resp.access_token,
            expires_at: Instant::now() + Duration::from_secs(token_resp.expires_in),
        })
    }
}
