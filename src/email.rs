// Email notifications via Microsoft Graph API.
// Uses the same app registration as the calendar integration.
// Requires Mail.Send application permission in Azure AD.

use anyhow::{Context, Result};
use serde::Serialize;

use crate::db::DbPool;
use crate::models::VisitDetail;

/// Graph email configuration — reuses calendar credentials
#[derive(Debug, Clone)]
pub struct EmailConfig {
    pub from_address: String,
    pub from_name: String,
    pub receptionist_email: String,
}

impl EmailConfig {
    pub fn from_db(db: &DbPool) -> Option<Self> {
        let get = |key: &str| crate::db::get_setting(db, key).filter(|s| !s.is_empty());
        let from_address = get("smtp_from_address")?;
        let from_name = get("smtp_from_name").unwrap_or_else(|| "GateKeeper".to_string());
        let receptionist_email = get("receptionist_email").unwrap_or_default();
        Some(Self { from_address, from_name, receptionist_email })
    }
}

// ── Graph sendMail types ──────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SendMailRequest {
    message: GraphMessage,
    save_to_sent_items: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphMessage {
    subject: String,
    body: MessageBody,
    from: Recipient,
    to_recipients: Vec<Recipient>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MessageBody {
    content_type: String,
    content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Recipient {
    email_address: EmailAddr,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailAddr {
    address: String,
    name: String,
}

// ── Send via Graph API ────────────────────────────────────────

async fn send_graph_email(
    db: &DbPool,
    from: &EmailConfig,
    to_address: &str,
    to_name: &str,
    subject: &str,
    html_body: &str,
) -> Result<()> {
    // Get a token from the existing Graph auth system
    let token = get_graph_token(db).await?;

    let request = SendMailRequest {
        message: GraphMessage {
            subject: subject.to_string(),
            body: MessageBody {
                content_type: "HTML".to_string(),
                content: html_body.to_string(),
            },
            from: Recipient {
                email_address: EmailAddr {
                    address: from.from_address.clone(),
                    name: from.from_name.clone(),
                },
            },
            to_recipients: vec![Recipient {
                email_address: EmailAddr {
                    address: to_address.to_string(),
                    name: to_name.to_string(),
                },
            }],
        },
        save_to_sent_items: false,
    };

    let url = format!(
        "https://graph.microsoft.com/v1.0/users/{}/sendMail",
        from.from_address
    );

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(&token)
        .json(&request)
        .send()
        .await
        .context("Graph sendMail request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Graph sendMail error {status}: {body}");
    }

    Ok(())
}

/// Get a Graph API token using environment variables only
async fn get_graph_token(_db: &DbPool) -> Result<String> {
    let tenant_id = std::env::var("GRAPH_TENANT_ID")
        .context("GRAPH_TENANT_ID env var not set")?;
    let client_id = std::env::var("GRAPH_CLIENT_ID")
        .context("GRAPH_CLIENT_ID env var not set")?;
    let client_secret = std::env::var("GRAPH_CLIENT_SECRET")
        .context("GRAPH_CLIENT_SECRET env var not set")?;

    let url = format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
        tenant_id
    );

    let params = [
        ("grant_type", "client_credentials"),
        ("client_id", &client_id),
        ("client_secret", &client_secret),
        ("scope", "https://graph.microsoft.com/.default"),
    ];

    let client = reqwest::Client::new();
    let resp = client
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

    #[derive(serde::Deserialize)]
    struct TokenResp {
        access_token: String,
    }

    let token: TokenResp = resp.json().await.context("Failed to parse token")?;
    Ok(token.access_token)
}

// ── Email content builders ────────────────────────────────────

fn host_arrival_email(visit: &VisitDetail) -> (String, String) {
    use crate::templates::html_escape;
    let company = visit.visitor.company.as_deref().unwrap_or("—");
    let areas = visit.areas_requested.as_deref().unwrap_or("Lobby");

    let subject = format!(
        "Visitor Arrival: {} is here to see you",
        visit.visitor.name
    );

    let body = format!(
        r#"<div style="font-family:Arial,sans-serif;max-width:500px;">
<h2 style="color:#1a56db;margin-bottom:8px;">Visitor Arrival</h2>
<p>A visitor has arrived to see you:</p>
<table style="font-size:14px;border-collapse:collapse;width:100%;margin:16px 0;">
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Visitor</td>
        <td style="padding:6px 12px;">{name}</td></tr>
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Company</td>
        <td style="padding:6px 12px;">{company}</td></tr>
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Purpose</td>
        <td style="padding:6px 12px;">{purpose}</td></tr>
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Areas</td>
        <td style="padding:6px 12px;">{areas}</td></tr>
</table>
<p style="color:#666;">Please proceed to reception to meet your visitor.</p>
</div>"#,
        name = html_escape(&visit.visitor.name),
        company = html_escape(company),
        purpose = html_escape(&visit.purpose),
        areas = html_escape(areas),
    );

    (subject, body)
}

fn visitor_confirmation_email(visit: &VisitDetail) -> (String, String) {
    use crate::templates::html_escape;
    let date = visit.expected_date.as_deref().unwrap_or("TBD");
    let areas = visit.areas_requested.as_deref().unwrap_or("Lobby");

    let subject = format!("Visit Confirmation — {}", date);

    let body = format!(
        r#"<div style="font-family:Arial,sans-serif;max-width:500px;">
<h2 style="color:#1a56db;margin-bottom:8px;">Visit Confirmation</h2>
<p>Your visit has been registered. Here are the details:</p>
<table style="font-size:14px;border-collapse:collapse;width:100%;margin:16px 0;">
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Date</td>
        <td style="padding:6px 12px;">{date}</td></tr>
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Host</td>
        <td style="padding:6px 12px;">{host} ({dept})</td></tr>
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Purpose</td>
        <td style="padding:6px 12px;">{purpose}</td></tr>
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Areas</td>
        <td style="padding:6px 12px;">{areas}</td></tr>
</table>
<p>Please check in at reception upon arrival. Bring a valid government-issued photo ID.</p>
<p style="color:#666;font-size:12px;">This is an automated message.</p>
</div>"#,
        date = html_escape(date),
        host = html_escape(&visit.host.name),
        dept = html_escape(&visit.host.department),
        purpose = html_escape(&visit.purpose),
        areas = html_escape(areas),
    );

    (subject, body)
}

fn receptionist_email(visit: &VisitDetail, is_walk_in: bool) -> (String, String) {
    use crate::templates::html_escape;
    let visit_type = if is_walk_in { "Walk-In" } else { "Pre-Registration" };
    let company = visit.visitor.company.as_deref().unwrap_or("—");
    let date = visit.expected_date.as_deref().unwrap_or("Today");
    let areas = visit.areas_requested.as_deref().unwrap_or("Lobby");

    let subject = format!(
        "{}: {} visiting {}",
        visit_type, visit.visitor.name, visit.host.name
    );

    let body = format!(
        r#"<div style="font-family:Arial,sans-serif;max-width:500px;">
<h2 style="color:#1a56db;margin-bottom:8px;">New {visit_type}</h2>
<table style="font-size:14px;border-collapse:collapse;width:100%;margin:16px 0;">
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Visitor</td>
        <td style="padding:6px 12px;">{name}</td></tr>
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Company</td>
        <td style="padding:6px 12px;">{company}</td></tr>
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Host</td>
        <td style="padding:6px 12px;">{host}</td></tr>
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Date</td>
        <td style="padding:6px 12px;">{date}</td></tr>
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Purpose</td>
        <td style="padding:6px 12px;">{purpose}</td></tr>
    <tr><td style="padding:6px 12px;font-weight:bold;background:#f3f4f6;">Areas</td>
        <td style="padding:6px 12px;">{areas}</td></tr>
</table>
</div>"#,
        visit_type = visit_type,
        name = html_escape(&visit.visitor.name),
        company = html_escape(company),
        host = html_escape(&visit.host.name),
        date = html_escape(date),
        purpose = html_escape(&visit.purpose),
        areas = html_escape(areas),
    );

    (subject, body)
}

// ── Public API ────────────────────────────────────────────────

/// Send all relevant emails for a new pre-registration
pub async fn send_preregistration_emails(
    db: &DbPool,
    visit: &VisitDetail,
    visitor_email: Option<&str>,
) {
    let config = match EmailConfig::from_db(db) {
        Some(c) => c,
        None => {
            tracing::debug!("Email not configured (no from address), skipping");
            return;
        }
    };

    // Notify receptionist
    if !config.receptionist_email.is_empty() {
        let (subj, body) = receptionist_email(visit, false);
        if let Err(e) = send_graph_email(
            db, &config, &config.receptionist_email, "Front Desk", &subj, &body,
        ).await {
            tracing::warn!("Failed to email receptionist: {e:#}");
        }
    }

    // Confirm to visitor
    if let Some(email) = visitor_email {
        let (subj, body) = visitor_confirmation_email(visit);
        if let Err(e) = send_graph_email(
            db, &config, email, &visit.visitor.name, &subj, &body,
        ).await {
            tracing::warn!("Failed to email visitor: {e:#}");
        }
    }

    // Notify host
    let (subj, body) = host_arrival_email(visit);
    if let Err(e) = send_graph_email(
        db, &config, &visit.host.email, &visit.host.name, &subj, &body,
    ).await {
        tracing::warn!("Failed to email host: {e:#}");
    }
}

/// Send all relevant emails for a walk-in
pub async fn send_walkin_emails(db: &DbPool, visit: &VisitDetail) {
    let config = match EmailConfig::from_db(db) {
        Some(c) => c,
        None => return,
    };

    // Notify receptionist
    if !config.receptionist_email.is_empty() {
        let (subj, body) = receptionist_email(visit, true);
        if let Err(e) = send_graph_email(
            db, &config, &config.receptionist_email, "Front Desk", &subj, &body,
        ).await {
            tracing::warn!("Failed to email receptionist: {e:#}");
        }
    }

    // Notify host
    let (subj, body) = host_arrival_email(visit);
    if let Err(e) = send_graph_email(
        db, &config, &visit.host.email, &visit.host.name, &subj, &body,
    ).await {
        tracing::warn!("Failed to email host: {e:#}");
    }
}

/// Send host arrival notification on check-in
pub async fn send_checkin_emails(db: &DbPool, visit: &VisitDetail) {
    let config = match EmailConfig::from_db(db) {
        Some(c) => c,
        None => return,
    };

    let (subj, body) = host_arrival_email(visit);
    if let Err(e) = send_graph_email(
        db, &config, &visit.host.email, &visit.host.name, &subj, &body,
    ).await {
        tracing::warn!("Failed to email host on check-in: {e:#}");
    }
}

/// Send a test email (used by admin panel)
pub async fn send_test_email(db: &DbPool, to_address: &str) -> Result<()> {
    let config = EmailConfig::from_db(db)
        .context("Email not configured (set from address in admin panel)")?;

    send_graph_email(
        db,
        &config,
        to_address,
        "Test Recipient",
        "GateKeeper Test Email",
        "<div style='font-family:Arial,sans-serif;'>\
         <h2 style='color:#1a56db;'>GateKeeper Email Test</h2>\
         <p>If you see this, email via Microsoft Graph is working correctly.</p>\
         </div>",
    ).await
}
