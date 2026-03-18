// Bridge between GateKeeper's SQLite visitor records and the Graph calendar API.
// Call `schedule_visit` after a check-in — it creates the O365 event and writes
// the event_id back to the visits table.

use anyhow::Result;
use chrono::{DateTime, Local};
use rusqlite::params;

use crate::db::DbPool;
use crate::graph::{build_from_env, CalendarClient, VisitorEvent};

/// Subset of a GateKeeper visit needed for calendar scheduling
pub struct VisitRecord {
    pub visit_id: String,
    pub visitor_name: String,
    pub visitor_email: Option<String>,
    pub host_email: String,
    pub host_name: Option<String>,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub location: String,
    pub reason: String,
    pub reminder_minutes: Option<u32>,
}

pub struct GraphService {
    client: CalendarClient,
    receptionist_email: Option<String>,
}

impl GraphService {
    /// Build from environment variables. Returns Err if GRAPH_ vars are missing.
    pub fn from_env() -> Result<Self> {
        let env = build_from_env()?;
        Ok(Self {
            client: env.client,
            receptionist_email: env.receptionist_email,
        })
    }

    /// Create calendar event for a visit, then write event_id + weblink back
    pub async fn schedule_visit(
        &self,
        db: &DbPool,
        visit: &VisitRecord,
    ) -> Result<()> {
        let event = VisitorEvent {
            subject: None,
            visitor_name: visit.visitor_name.clone(),
            visitor_email: visit.visitor_email.clone(),
            host_email: visit.host_email.clone(),
            host_name: visit.host_name.clone(),
            start: visit.start,
            end: visit.end,
            location: visit.location.clone(),
            reason_for_visit: visit.reason.clone(),
            reminder_minutes: Some(visit.reminder_minutes.unwrap_or(15)),
            additional_attendees: self.receptionist_email.iter()
                .map(|email| (email.clone(), Some("Front Desk".to_string())))
                .collect(),
        };

        // Make the API call (this is the async part)
        let created = self.client.create_visitor_event(&event).await?;

        // Brief lock to write back the event ID
        {
            let conn = db.lock().unwrap();
            conn.execute(
                "UPDATE visits SET
                   graph_event_id = ?1,
                   graph_web_link = ?2,
                   calendar_status = 'scheduled',
                   updated_at = datetime('now')
                 WHERE id = ?3",
                params![created.id, created.web_link, visit.visit_id],
            )?;
        }

        tracing::info!(
            visit_id = %visit.visit_id,
            event_id = %created.id,
            visitor = %visit.visitor_name,
            "Calendar event created"
        );

        Ok(())
    }

    /// Cancel a visit: delete Graph event + update DB status
    pub async fn cancel_visit(
        &self,
        db: &DbPool,
        visit_id: &str,
    ) -> Result<()> {
        let event_id: Option<String> = {
            let conn = db.lock().unwrap();
            conn.query_row(
                "SELECT graph_event_id FROM visits WHERE id = ?1",
                params![visit_id],
                |row| row.get(0),
            )
            .ok()
        };

        if let Some(eid) = event_id {
            self.client.delete_event(&eid).await?;
        }

        {
            let conn = db.lock().unwrap();
            conn.execute(
                "UPDATE visits SET \
                   calendar_status = 'cancelled', \
                   updated_at = datetime('now') \
                 WHERE id = ?1",
                params![visit_id],
            )?;
        }

        Ok(())
    }
}
