// Creates events on an O365 Group Calendar via Microsoft Graph API.
// POST /v1.0/groups/{group-id}/calendar/events

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use super::auth::TokenProvider;

// --- Request Types ---

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateEventRequest {
    subject: String,
    body: EventBody,
    start: DateTimeTimeZone,
    end: DateTimeTimeZone,
    location: Location,
    attendees: Vec<Attendee>,
    is_reminder_on: bool,
    reminder_minutes_before_start: u32,
    show_as: ShowAs,
    is_online_meeting: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EventBody {
    content_type: String,
    content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DateTimeTimeZone {
    date_time: String,
    time_zone: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Location {
    display_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Attendee {
    email_address: EmailAddress,
    #[serde(rename = "type")]
    attendee_type: AttendeeType,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EmailAddress {
    address: String,
    name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum AttendeeType {
    Required,
    Optional,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum ShowAs {
    Busy,
}

// --- Response Types ---

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CreatedEvent {
    pub id: String,
    pub subject: String,
    #[serde(rename = "webLink")]
    pub web_link: Option<String>,
}

// --- Public API ---

/// Everything needed to create a visitor calendar event
#[derive(Debug, Clone)]
pub struct VisitorEvent {
    pub subject: Option<String>,
    pub visitor_name: String,
    pub visitor_email: Option<String>,
    pub host_email: String,
    pub host_name: Option<String>,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub location: String,
    pub reason_for_visit: String,
    pub reminder_minutes: Option<u32>,
    pub additional_attendees: Vec<(String, Option<String>)>,
}

pub struct CalendarClient {
    token_provider: TokenProvider,
    http: reqwest::Client,
    group_id: String,
}

impl CalendarClient {
    pub fn new(
        token_provider: TokenProvider,
        http: reqwest::Client,
        group_id: String,
    ) -> Self {
        Self { token_provider, http, group_id }
    }

    /// Create a visitor event on the group calendar.
    /// Returns the Graph event ID and web link for writeback to GateKeeper DB.
    pub async fn create_visitor_event(
        &self,
        event: &VisitorEvent,
    ) -> Result<CreatedEvent> {
        let token = self.token_provider.get_token().await?;
        let request = self.build_request(event);

        let url = format!(
            "https://graph.microsoft.com/v1.0/groups/{}/calendar/events",
            self.group_id
        );

        let resp = self.http
            .post(&url)
            .bearer_auth(&token)
            .json(&request)
            .send()
            .await
            .context("Graph API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Graph create event error {status}: {body}");
        }

        let created: CreatedEvent = resp.json().await
            .context("Failed to parse created event response")?;

        Ok(created)
    }

    /// Delete a previously created event (for cancellations)
    pub async fn delete_event(&self, event_id: &str) -> Result<()> {
        let token = self.token_provider.get_token().await?;

        let url = format!(
            "https://graph.microsoft.com/v1.0/groups/{}/calendar/events/{}",
            self.group_id, event_id
        );

        let resp = self.http
            .delete(&url)
            .bearer_auth(&token)
            .send()
            .await
            .context("Graph delete request failed")?;

        if !resp.status().is_success() && resp.status().as_u16() != 204 {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Graph delete event error {status}: {body}");
        }

        Ok(())
    }

    fn build_request(&self, event: &VisitorEvent) -> CreateEventRequest {
        let subject = event.subject.clone().unwrap_or_else(|| {
            format!(
                "Visitor: {} -- {}",
                event.visitor_name, event.reason_for_visit
            )
        });

        // Truncate subject to 255 chars (Graph limit)
        let subject = if subject.len() > 255 {
            format!("{}...", &subject[..252])
        } else {
            subject
        };

        let body_html = format!(
            r#"<table style="font-family:Arial,sans-serif;font-size:14px;">
  <tr><td style="padding:4px 12px 4px 0;font-weight:bold;">Visitor</td>
      <td>{visitor_name}</td></tr>
  <tr><td style="padding:4px 12px 4px 0;font-weight:bold;">Host</td>
      <td>{host}</td></tr>
  <tr><td style="padding:4px 12px 4px 0;font-weight:bold;">Location</td>
      <td>{location}</td></tr>
  <tr><td style="padding:4px 12px 4px 0;font-weight:bold;">Reason</td>
      <td>{reason}</td></tr>
</table>"#,
            visitor_name = htmlescape(&event.visitor_name),
            host = htmlescape(
                event.host_name.as_deref().unwrap_or(&event.host_email)
            ),
            location = htmlescape(&event.location),
            reason = htmlescape(&event.reason_for_visit),
        );

        let tz = "Eastern Standard Time".to_string();

        let mut attendees = vec![Attendee {
            email_address: EmailAddress {
                address: event.host_email.clone(),
                name: event.host_name.clone(),
            },
            attendee_type: AttendeeType::Required,
        }];

        if let Some(ref visitor_email) = event.visitor_email {
            attendees.push(Attendee {
                email_address: EmailAddress {
                    address: visitor_email.clone(),
                    name: Some(event.visitor_name.clone()),
                },
                attendee_type: AttendeeType::Required,
            });
        }

        for (email, name) in &event.additional_attendees {
            attendees.push(Attendee {
                email_address: EmailAddress {
                    address: email.clone(),
                    name: name.clone(),
                },
                attendee_type: AttendeeType::Optional,
            });
        }

        CreateEventRequest {
            subject,
            body: EventBody {
                content_type: "HTML".to_string(),
                content: body_html,
            },
            start: DateTimeTimeZone {
                date_time: event.start.format("%Y-%m-%dT%H:%M:%S").to_string(),
                time_zone: tz.clone(),
            },
            end: DateTimeTimeZone {
                date_time: event.end.format("%Y-%m-%dT%H:%M:%S").to_string(),
                time_zone: tz,
            },
            location: Location {
                display_name: event.location.clone(),
            },
            attendees,
            is_reminder_on: true,
            reminder_minutes_before_start: event.reminder_minutes.unwrap_or(15),
            show_as: ShowAs::Busy,
            is_online_meeting: false,
        }
    }
}

/// Minimal HTML escaping for event body content
fn htmlescape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
