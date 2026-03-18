pub mod auth;
pub mod calendar;

pub use auth::{GraphConfig, TokenProvider};
pub use calendar::{CalendarClient, CreatedEvent, VisitorEvent};

use anyhow::Result;

/// Result of building from env: the calendar client + any extra config
pub struct GraphEnv {
    pub client: CalendarClient,
    pub receptionist_email: Option<String>,
}

/// Convenience: build a CalendarClient from environment config
pub fn build_from_env() -> Result<GraphEnv> {
    let config = GraphConfig::from_env()?;
    let group_id = config.group_id.clone();
    let receptionist_email = config.receptionist_email.clone();

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("GateKeeper/1.0")
        .build()?;

    let token_provider = TokenProvider::new(config, http.clone());
    Ok(GraphEnv {
        client: CalendarClient::new(token_provider, http, group_id),
        receptionist_email,
    })
}
