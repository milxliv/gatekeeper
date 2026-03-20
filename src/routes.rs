use std::sync::Arc;
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    Form, Json,
};
use chrono::TimeZone;
use serde::{Deserialize, Serialize};
use crate::db;
use crate::graph_service::VisitRecord;
use crate::models::*;
use crate::templates;
use crate::AppState;

/// Log internal error details and return a safe user-facing message
fn safe_error(context: &str, err: impl std::fmt::Display) -> String {
    tracing::error!("{}: {}", context, err);
    format!("{} — please try again or contact an administrator.", context)
}

// ── Auth handlers ─────────────────────────────────────────────

pub async fn page_login(State(state): State<Arc<AppState>>) -> Html<String> {
    // If no password set, redirect to dashboard
    if state.password_hash.is_none() {
        return Html(r#"<script>window.location='/';</script>"#.to_string());
    }
    Html(templates::login_page(None))
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub password: String,
}

pub async fn api_login(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    if state.password_hash.is_none() {
        return axum::response::Redirect::to("/").into_response();
    }

    use argon2::{Argon2, PasswordVerifier, PasswordHash};

    let verify = |hash: &str| -> bool {
        PasswordHash::new(hash)
            .map(|parsed| Argon2::default().verify_password(form.password.as_bytes(), &parsed).is_ok())
            .unwrap_or(false)
    };

    // Check admin password first, then front desk password
    let role = if let Some(ref admin_hash) = state.admin_password_hash {
        if verify(admin_hash) {
            Some("admin")
        } else if state.password_hash.as_deref().is_some_and(|h| verify(h)) {
            Some("user")
        } else {
            None
        }
    } else if state.password_hash.as_deref().is_some_and(|h| verify(h)) {
        // No separate admin password — front desk password grants admin
        Some("admin")
    } else {
        None
    };

    match role {
        Some(role) => {
            let token = uuid::Uuid::new_v4().to_string();
            let _ = db::create_session(&state.db, &token, role, 24);
            // Use Secure flag only when behind HTTPS (e.g. Cloudflare tunnel)
            let secure = if headers.get("x-forwarded-proto")
                .and_then(|v| v.to_str().ok()) == Some("https") { "; Secure" } else { "" };
            let cookie = format!(
                "gk_session={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=86400{}",
                token, secure
            );
            (
                [(axum::http::header::SET_COOKIE, cookie)],
                axum::response::Redirect::to("/"),
            ).into_response()
        }
        None => {
            Html(templates::login_page(Some("Invalid password."))).into_response()
        }
    }
}

pub async fn api_logout(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    // Extract and delete session
    if let Some(cookie_header) = headers.get("cookie") {
        if let Ok(cookies) = cookie_header.to_str() {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix("gk_session=") {
                    db::delete_session(&state.db, token);
                }
            }
        }
    }
    let clear_cookie = "gk_session=; Path=/; HttpOnly; Max-Age=0";
    (
        [(axum::http::header::SET_COOKIE, clear_cookie)],
        axum::response::Redirect::to("/login"),
    )
}

// ── Theme & role helpers ──────────────────────────────────────

fn apply_theme(state: &AppState) {
    let theme = db::get_setting(&state.db, "ui_theme")
        .unwrap_or_else(|| "system".to_string());
    templates::set_theme(&theme);
}

fn apply_role(role: &crate::UserRole) {
    templates::set_role(&role.0);
}

// ── Page routes ───────────────────────────────────────────────

pub async fn page_dashboard(
    State(state): State<Arc<AppState>>,
    axum::Extension(role): axum::Extension<crate::UserRole>,
) -> Html<String> {
    apply_theme(&state);
    apply_role(&role);
    let visits = db::list_visits_today(&state.db).unwrap_or_default();
    let upcoming = db::list_preregistered_upcoming(&state.db).unwrap_or_default();
    let graph_connected = state.graph.is_some();
    Html(templates::dashboard_page(&visits, &upcoming, graph_connected))
}

pub async fn page_pre_register(
    State(state): State<Arc<AppState>>,
    axum::Extension(role): axum::Extension<crate::UserRole>,
) -> Html<String> {
    apply_theme(&state);
    apply_role(&role);
    let hosts = db::list_hosts(&state.db).unwrap_or_default();
    let purposes = db::get_setting(&state.db, "purpose_list")
        .unwrap_or_else(|| "Meeting,Sales Call,Interview,Vendor / Install,Tour,Delivery".to_string());
    let areas = db::get_setting(&state.db, "area_list")
        .unwrap_or_else(|| "Studios,Master Control,Rack Room,Transmitter,Newsroom,Offices,Multiple Areas".to_string());
    let visitor_types = db::get_setting(&state.db, "visitor_type_list")
        .unwrap_or_else(|| "Visitor,Guest,Contractor,Vendor,Interview".to_string());
    Html(templates::pre_register_page(&hosts, &purposes, &areas, &visitor_types))
}

pub async fn page_walk_in(
    State(state): State<Arc<AppState>>,
    axum::Extension(role): axum::Extension<crate::UserRole>,
) -> Html<String> {
    apply_theme(&state);
    apply_role(&role);
    let hosts = db::list_hosts(&state.db).unwrap_or_default();
    let areas = db::get_setting(&state.db, "area_list")
        .unwrap_or_else(|| "Studios,Master Control,Rack Room,Transmitter,Newsroom,Offices,Multiple Areas".to_string());
    let visitor_types = db::get_setting(&state.db, "visitor_type_list")
        .unwrap_or_else(|| "Visitor,Guest,Contractor,Vendor,Interview".to_string());
    Html(templates::walk_in_page(&hosts, &areas, &visitor_types))
}

pub async fn page_hosts(
    State(state): State<Arc<AppState>>,
    axum::Extension(role): axum::Extension<crate::UserRole>,
) -> Html<String> {
    apply_theme(&state);
    apply_role(&role);
    let hosts = db::list_hosts(&state.db).unwrap_or_default();
    Html(templates::hosts_page(&hosts))
}

pub async fn page_log(
    State(state): State<Arc<AppState>>,
    axum::Extension(role): axum::Extension<crate::UserRole>,
) -> Html<String> {
    apply_theme(&state);
    apply_role(&role);
    let visits = db::search_visits(&state.db, "", None, None).unwrap_or_default();
    Html(templates::log_page(&visits))
}

// ── API: HTMX partials ───────────────────────────────────────

/// Dashboard auto-refresh (every 30s)
pub async fn api_dashboard_today(
    State(state): State<Arc<AppState>>,
) -> Html<String> {
    let visits = db::list_visits_today(&state.db).unwrap_or_default();
    let rows = if visits.is_empty() {
        "<p style='color:var(--text-dim);padding:1rem;'>No visitors today.</p>"
            .to_string()
    } else {
        render_full_table(&visits, true)
    };
    Html(rows)
}

/// Pre-register a visitor
pub async fn api_pre_register(
    State(state): State<Arc<AppState>>,
    Form(form): Form<PreRegisterForm>,
) -> Html<String> {
    if form.visitor_name.trim().is_empty() || form.host_id.trim().is_empty() {
        return Html(templates::alert_error("Name and host are required."));
    }

    let visitor = NewVisitor {
        name: form.visitor_name.trim().to_string(),
        company: non_empty(form.visitor_company),
        phone: non_empty(form.visitor_phone),
        email: non_empty(form.visitor_email),
        notes: None,
    };

    let visitor_id = match db::find_or_create_visitor(&state.db, &visitor) {
        Ok(id) => id,
        Err(e) => {
            return Html(templates::alert_error(
                &format!("Database error: {}", e),
            ))
        }
    };

    let duration_mins: Option<i32> = form.duration.as_deref()
        .and_then(|d| d.parse().ok());
    let expected_time_str = non_empty(form.expected_time);
    let visitor_type = form.visitor_type
        .unwrap_or_else(|| "Visitor".to_string());
    let visit = NewVisit {
        visitor_id,
        host_id: form.host_id,
        purpose: form.purpose,
        areas_requested: non_empty(form.areas_requested),
        special_notes: non_empty(form.special_notes),
        visitor_type,
        status: "pending".to_string(),
        pre_registered: true,
        expected_date: Some(form.expected_date),
        expected_time: expected_time_str.clone(),
        duration_minutes: duration_mins,
        is_group: false,
        group_name: None,
        group_size: None,
    };

    match db::create_visit(&state.db, &visit) {
        Ok(visit_id) => {
            let host = db::get_host(&state.db, &visit.host_id).ok().flatten();
            let host_name = host.as_ref()
                .map(|h| h.name.clone())
                .unwrap_or_else(|| "the host".to_string());

            // Schedule calendar event if Graph is enabled
            if let (Some(ref graph), Some(ref host)) = (&state.graph, &host) {
                let expected = visit.expected_date.clone()
                    .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());

                // Parse time from form (default 9:00 AM)
                let time_str = expected_time_str.as_deref().unwrap_or("09:00");
                let (start_h, start_m) = {
                    let parts: Vec<&str> = time_str.split(':').collect();
                    (
                        parts.first().and_then(|p| p.parse::<u32>().ok()).unwrap_or(9),
                        parts.get(1).and_then(|p| p.parse::<u32>().ok()).unwrap_or(0),
                    )
                };
                let duration_mins: i64 = form.duration.as_deref()
                    .and_then(|d| d.parse().ok())
                    .unwrap_or(60);

                let start = chrono::NaiveDate::parse_from_str(&expected, "%Y-%m-%d")
                    .ok()
                    .and_then(|d| d.and_hms_opt(start_h, start_m, 0))
                    .map(|dt| {
                        chrono::Local::now()
                            .timezone()
                            .from_local_datetime(&dt)
                            .single()
                    })
                    .flatten()
                    .unwrap_or_else(chrono::Local::now);

                let end = start + chrono::Duration::minutes(duration_mins);

                let record = VisitRecord {
                    visit_id: visit_id.clone(),
                    visitor_name: visitor.name.clone(),
                    visitor_email: visitor.email.clone(),
                    host_email: host.email.clone(),
                    host_name: Some(host.name.clone()),
                    start,
                    end,
                    location: visit.areas_requested.clone()
                        .unwrap_or_else(|| "Main Lobby".to_string()),
                    reason: visit.purpose.clone(),
                    reminder_minutes: Some(15),
                };

                let graph = Arc::clone(graph);
                let db = state.db.clone();
                tokio::spawn(async move {
                    if let Err(e) = graph.schedule_visit(&db, &record).await {
                        tracing::error!(
                            visit_id = %record.visit_id,
                            "Graph calendar scheduling failed: {e:#}"
                        );
                    }
                });
            }

            // Send email notifications in background
            {
                let db = state.db.clone();
                let visitor_email = visitor.email.clone();
                // Build a VisitDetail for the email templates
                if let Ok(visits) = db::list_visits_today(&db) {
                    if let Some(detail) = visits.iter().find(|v| v.id == visit_id) {
                        let detail = detail.clone();
                        let db2 = db.clone();
                        tokio::spawn(async move {
                            crate::email::send_preregistration_emails(
                                &db2,
                                &detail,
                                visitor_email.as_deref(),
                            ).await;
                        });
                    }
                }
            }

            Html(templates::alert_success(&format!(
                "Pre-registered. {} will be notified when they arrive.",
                host_name
            )))
        }
        Err(e) => {
            Html(templates::alert_error(&safe_error("Failed to register visitor", e)))
        }
    }
}

/// Walk-in check-in (triggers notification)
pub async fn api_walk_in(
    State(state): State<Arc<AppState>>,
    Form(form): Form<WalkInForm>,
) -> Html<String> {
    if form.visitor_name.trim().is_empty() || form.host_id.trim().is_empty() {
        return Html(templates::alert_error("Name and host are required."));
    }

    let visitor = NewVisitor {
        name: form.visitor_name.trim().to_string(),
        company: non_empty(form.visitor_company),
        phone: non_empty(form.visitor_phone),
        email: None,
        notes: None,
    };

    let visitor_id = match db::find_or_create_visitor(&state.db, &visitor) {
        Ok(id) => id,
        Err(e) => {
            return Html(templates::alert_error(
                &format!("Database error: {}", e),
            ))
        }
    };

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let visitor_type = form.visitor_type
        .unwrap_or_else(|| "Visitor".to_string());
    let visit = NewVisit {
        visitor_id,
        host_id: form.host_id.clone(),
        purpose: form.purpose,
        areas_requested: non_empty(form.areas_requested),
        special_notes: non_empty(form.special_notes),
        visitor_type,
        status: "pending".to_string(),
        pre_registered: false,
        expected_date: Some(today),
        expected_time: None,
        duration_minutes: None,
        is_group: false,
        group_name: None,
        group_size: None,
    };

    match db::create_visit(&state.db, &visit) {
        Ok(_id) => {
            let host = db::get_host(&state.db, &form.host_id).ok().flatten();
            let host_name =
                host.as_ref().map(|h| h.name.as_str()).unwrap_or("the host");
            let host_email =
                host.as_ref().map(|h| h.email.as_str()).unwrap_or("unknown");
            let host_phone = host
                .as_ref()
                .and_then(|h| h.phone.as_deref())
                .unwrap_or("no phone");

            tracing::info!(
                "WALK-IN ALERT: {} from {} is here to see {} ({} / {})",
                visitor.name,
                visitor.company.as_deref().unwrap_or("unknown company"),
                host_name,
                host_email,
                host_phone
            );

            // Send email notifications in background
            {
                let db = state.db.clone();
                if let Ok(visits) = db::list_visits_today(&db) {
                    if let Some(detail) = visits.iter().find(|v| v.id == _id) {
                        let detail = detail.clone();
                        let db2 = db.clone();
                        tokio::spawn(async move {
                            crate::email::send_walkin_emails(&db2, &detail).await;
                        });
                    }
                }
            }

            Html(templates::alert_success(&format!(
                "Walk-in logged. Notification sent to {} ({}). \
                 Visitor should wait for approval.",
                host_name, host_email
            )))
        }
        Err(e) => {
            Html(templates::alert_error(&safe_error("Failed to check in", e)))
        }
    }
}

// ── Group Visit ─────────────────────────────────────────────

pub async fn page_group_visit(
    State(state): State<Arc<AppState>>,
    axum::Extension(role): axum::Extension<crate::UserRole>,
) -> Html<String> {
    apply_theme(&state);
    apply_role(&role);
    let hosts = db::list_hosts(&state.db).unwrap_or_default();
    let purposes = db::get_setting(&state.db, "purpose_list")
        .unwrap_or_else(|| "Meeting,Sales Call,Interview,Vendor / Install,Tour,Delivery".to_string());
    let areas = db::get_setting(&state.db, "area_list")
        .unwrap_or_else(|| "Studios,Master Control,Rack Room,Transmitter,Newsroom,Offices,Multiple Areas".to_string());
    let visitor_types = db::get_setting(&state.db, "visitor_type_list")
        .unwrap_or_else(|| "Visitor,Guest,Contractor,Vendor,Interview".to_string());
    Html(templates::group_visit_page(&hosts, &purposes, &areas, &visitor_types))
}

pub async fn api_group_visit(
    State(state): State<Arc<AppState>>,
    Form(form): Form<GroupVisitForm>,
) -> Html<String> {
    let group_name = form.group_name.trim().to_string();
    if group_name.is_empty() || form.host_id.trim().is_empty() {
        return Html(templates::alert_error("Group name and host are required."));
    }
    if form.group_size < 2 || form.group_size > 200 {
        return Html(templates::alert_error("Group size must be between 2 and 200."));
    }

    // Create a synthetic visitor record for the group
    let visitor = NewVisitor {
        name: group_name.clone(),
        company: None,
        phone: None,
        email: None,
        notes: Some(format!("Group of {}", form.group_size)),
    };

    let visitor_id = match db::find_or_create_visitor(&state.db, &visitor) {
        Ok(id) => id,
        Err(e) => {
            return Html(templates::alert_error(&safe_error("Database error", e)));
        }
    };

    let duration_mins: Option<i32> = form.duration.as_deref()
        .and_then(|d| d.parse().ok());
    let visitor_type = form.visitor_type
        .unwrap_or_else(|| "Visitor".to_string());
    let visit = NewVisit {
        visitor_id,
        host_id: form.host_id,
        purpose: form.purpose,
        areas_requested: non_empty(form.areas_requested),
        special_notes: non_empty(form.special_notes),
        visitor_type,
        status: "pending".to_string(),
        pre_registered: true,
        expected_date: Some(form.expected_date),
        expected_time: non_empty(form.expected_time),
        duration_minutes: duration_mins,
        is_group: true,
        group_name: Some(group_name.clone()),
        group_size: Some(form.group_size),
    };

    match db::create_visit(&state.db, &visit) {
        Ok(_visit_id) => {
            let host = db::get_host(&state.db, &visit.host_id).ok().flatten();
            let host_name = host.as_ref()
                .map(|h| h.name.clone())
                .unwrap_or_else(|| "the host".to_string());

            Html(templates::alert_success(&format!(
                "Group \"{}\" ({} members) registered. {} will be notified.",
                group_name, form.group_size, host_name
            )))
        }
        Err(e) => {
            Html(templates::alert_error(&safe_error("Failed to register group", e)))
        }
    }
}

/// Add a new host
pub async fn api_add_host(
    State(state): State<Arc<AppState>>,
    Form(form): Form<NewHost>,
) -> Html<String> {
    if form.name.trim().is_empty() || form.email.trim().is_empty() {
        return Html(templates::alert_error("Name and email are required."));
    }
    match db::insert_host(&state.db, &form) {
        Ok(_) => Html(templates::alert_success(&format!(
            "{} added as host.",
            form.name
        ))),
        Err(e) => Html(templates::alert_error(&safe_error("Operation failed", e))),
    }
}

/// Update an existing host
pub async fn api_update_host(
    State(state): State<Arc<AppState>>,
    Path(host_id): Path<String>,
    Form(form): Form<NewHost>,
) -> Html<String> {
    if form.name.trim().is_empty() || form.email.trim().is_empty() {
        return Html(templates::alert_error("Name and email are required."));
    }
    match db::update_host(&state.db, &host_id, &form) {
        Ok(_) => Html(templates::alert_success(&format!(
            "{} updated.", form.name
        ))),
        Err(e) => Html(templates::alert_error(&safe_error("Operation failed", e))),
    }
}

/// Deactivate (soft-delete) a host
pub async fn api_delete_host(
    State(state): State<Arc<AppState>>,
    Path(host_id): Path<String>,
) -> Html<String> {
    match db::deactivate_host(&state.db, &host_id) {
        Ok(_) => Html(String::new()), // Row removed from DOM via hx-swap
        Err(e) => Html(templates::alert_error(&safe_error("Operation failed", e))),
    }
}

/// Approve a visit
pub async fn api_approve_visit(
    State(state): State<Arc<AppState>>,
    Path(visit_id): Path<String>,
) -> Html<String> {
    let _ = db::update_visit_status(&state.db, &visit_id, "approved");
    if let Ok(visits) = db::list_visits_today(&state.db) {
        if let Some(v) = visits.iter().find(|v| v.id == visit_id) {
            return Html(templates::visit_row_partial(v));
        }
    }
    Html("<tr><td colspan='9'>Updated</td></tr>".to_string())
}

/// Deny a visit — also cancels any Graph calendar event
pub async fn api_deny_visit(
    State(state): State<Arc<AppState>>,
    Path(visit_id): Path<String>,
) -> Html<String> {
    let _ = db::update_visit_status(&state.db, &visit_id, "denied");

    // Cancel calendar event in background if Graph is enabled
    if let Some(ref graph) = state.graph {
        let graph = Arc::clone(graph);
        let db = state.db.clone();
        let vid = visit_id.clone();
        tokio::spawn(async move {
            if let Err(e) = graph.cancel_visit(&db, &vid).await {
                tracing::error!(visit_id = %vid, "Graph cancel failed: {e:#}");
            }
        });
    }

    if let Ok(visits) = db::list_visits_today(&state.db) {
        if let Some(v) = visits.iter().find(|v| v.id == visit_id) {
            return Html(templates::visit_row_partial(v));
        }
    }
    Html("<tr><td colspan='9'>Denied</td></tr>".to_string())
}

/// Mark a visit as "running late" — push expected time forward by N minutes
#[derive(Debug, Deserialize)]
pub struct LateForm {
    pub delay_minutes: Option<i32>,
}

pub async fn api_late_visit(
    State(state): State<Arc<AppState>>,
    Path(visit_id): Path<String>,
    Form(form): Form<LateForm>,
) -> Html<String> {
    // Push expected_time forward by the delay; keep status as-is (pending)
    if let Some(mins) = form.delay_minutes {
        let _ = db::push_expected_time(&state.db, &visit_id, mins);
    }

    if let Ok(visits) = db::list_visits_today(&state.db) {
        if let Some(v) = visits.iter().find(|v| v.id == visit_id) {
            return Html(templates::visit_row_partial(v));
        }
    }
    Html("<tr><td colspan='9'>Updated</td></tr>".to_string())
}

/// Reschedule a visit to a new date/time
#[derive(Debug, Deserialize)]
pub struct RescheduleForm {
    pub new_date: String,
    pub new_time: Option<String>,
}

pub async fn api_reschedule_visit(
    State(state): State<Arc<AppState>>,
    Path(visit_id): Path<String>,
    Form(form): Form<RescheduleForm>,
) -> Html<String> {
    // reschedule_visit sets status to pending (today) or rescheduled (future)
    let _ = db::reschedule_visit(
        &state.db,
        &visit_id,
        &form.new_date,
        form.new_time.as_deref(),
    );

    // If rescheduled to today, return the updated row
    if let Ok(visits) = db::list_visits_today(&state.db) {
        if let Some(v) = visits.iter().find(|v| v.id == visit_id) {
            return Html(templates::visit_row_partial(v));
        }
    }
    // Rescheduled to a future date — row should disappear from today's table
    Html(String::new())
}

/// Check in a visitor — triggers Graph calendar event creation
pub async fn api_checkin_visit(
    State(state): State<Arc<AppState>>,
    Path(visit_id): Path<String>,
) -> Html<String> {
    let badge_num = db::next_badge_number(&state.db);
    let _ = db::check_in_visit(&state.db, &visit_id, Some(&badge_num));

    // Schedule calendar event in background if Graph is enabled
    if let Some(ref graph) = state.graph {
        let visit_detail = db::list_visits_today(&state.db)
            .ok()
            .and_then(|visits| {
                visits.into_iter().find(|v| v.id == visit_id)
            });

        if let Some(detail) = visit_detail {
            let now = chrono::Local::now();
            let record = VisitRecord {
                visit_id: detail.id.clone(),
                visitor_name: detail.visitor.name.clone(),
                visitor_email: None,
                host_email: detail.host.email.clone(),
                host_name: Some(detail.host.name.clone()),
                start: now,
                end: now + chrono::Duration::hours(1),
                location: detail
                    .areas_requested
                    .clone()
                    .unwrap_or_else(|| "Main Lobby".to_string()),
                reason: detail.purpose.clone(),
                reminder_minutes: Some(15),
            };

            let graph = Arc::clone(graph);
            let db = state.db.clone();
            tokio::spawn(async move {
                if let Err(e) = graph.schedule_visit(&db, &record).await {
                    tracing::error!(
                        visit_id = %record.visit_id,
                        "Graph calendar scheduling failed: {e:#}"
                    );
                }
            });
        }
    }

    // Send check-in email to host in background
    {
        let db = state.db.clone();
        let vid = visit_id.clone();
        tokio::spawn(async move {
            if let Ok(visits) = db::list_visits_today(&db) {
                if let Some(detail) = visits.iter().find(|v| v.id == vid) {
                    crate::email::send_checkin_emails(&db, detail).await;
                }
            }
        });
    }

    if let Ok(visits) = db::list_visits_today(&state.db) {
        if let Some(v) = visits.iter().find(|v| v.id == visit_id) {
            return Html(templates::visit_row_partial(v));
        }
    }
    Html("<tr><td colspan='9'>Checked in</td></tr>".to_string())
}

/// Check out a visitor
pub async fn api_checkout_visit(
    State(state): State<Arc<AppState>>,
    Path(visit_id): Path<String>,
) -> Html<String> {
    let _ = db::check_out_visit(&state.db, &visit_id);
    if let Ok(visits) = db::list_visits_today(&state.db) {
        if let Some(v) = visits.iter().find(|v| v.id == visit_id) {
            return Html(templates::visit_row_partial(v));
        }
    }
    Html("<tr><td colspan='9'>Checked out</td></tr>".to_string())
}

/// Check out all currently checked-in visitors at once
pub async fn api_checkout_all(
    State(state): State<Arc<AppState>>,
) -> Html<String> {
    let _ = db::check_out_all_today(&state.db);
    let visits = db::list_visits_today(&state.db).unwrap_or_default();
    let rows = if visits.is_empty() {
        "<p style='color:var(--text-dim);padding:1rem;'>No visitors today.</p>"
            .to_string()
    } else {
        render_full_table(&visits, true)
    };
    Html(rows)
}

/// Search the visitor log
pub async fn api_log_search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> Html<String> {
    let query = params.q.as_deref().unwrap_or("");
    let from = params.from.as_deref();
    let to = params.to.as_deref();
    let visits =
        db::search_visits(&state.db, query, from, to).unwrap_or_default();
    Html(render_full_table(&visits, false))
}

// ── Kiosk JSON API ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct KioskCheckInRequest {
    pub visitor_name: String,
    pub visitor_email: Option<String>,
    pub visitor_company: Option<String>,
    pub host_id: String,
    pub duration_minutes: Option<i64>,
    pub location: Option<String>,
    pub reason: String,
    pub reminder_minutes: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct KioskCheckInResponse {
    pub visit_id: String,
    pub calendar_scheduled: bool,
    pub message: String,
}

/// JSON check-in endpoint for kiosk / tablet UI
pub async fn api_kiosk_checkin(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<KioskCheckInRequest>,
) -> Result<Json<KioskCheckInResponse>, StatusCode> {
    // Verify kiosk secret if configured
    if let Some(ref secret) = state.kiosk_secret {
        let provided = headers
            .get("x-kiosk-secret")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided != secret {
            tracing::warn!("Kiosk check-in rejected: invalid or missing X-Kiosk-Secret header");
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    if req.visitor_name.trim().is_empty() || req.host_id.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let host = db::get_host(&state.db, &req.host_id)
        .ok()
        .flatten()
        .ok_or(StatusCode::NOT_FOUND)?;

    let visitor = NewVisitor {
        name: req.visitor_name.trim().to_string(),
        company: req.visitor_company.clone(),
        phone: None,
        email: req.visitor_email.clone(),
        notes: None,
    };

    let visitor_id = db::find_or_create_visitor(&state.db, &visitor)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let location = req.location.clone()
        .unwrap_or_else(|| "Main Lobby".to_string());
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let visit = NewVisit {
        visitor_id,
        host_id: req.host_id.clone(),
        purpose: req.reason.clone(),
        areas_requested: Some(location.clone()),
        special_notes: None,
        visitor_type: "Visitor".to_string(),
        status: "pending".to_string(),
        pre_registered: false,
        expected_date: Some(today),
        expected_time: None,
        duration_minutes: None,
        is_group: false,
        group_name: None,
        group_size: None,
    };

    let visit_id = db::create_visit(&state.db, &visit)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Schedule calendar event in background if Graph is enabled
    let mut calendar_scheduled = false;
    if let Some(ref graph) = state.graph {
        let now = chrono::Local::now();
        let duration = req.duration_minutes.unwrap_or(60);
        let end = now + chrono::Duration::minutes(duration);

        let record = VisitRecord {
            visit_id: visit_id.clone(),
            visitor_name: visitor.name.clone(),
            visitor_email: req.visitor_email.clone(),
            host_email: host.email.clone(),
            host_name: Some(host.name.clone()),
            start: now,
            end,
            location,
            reason: req.reason.clone(),
            reminder_minutes: req.reminder_minutes,
        };

        let graph = Arc::clone(graph);
        let db = state.db.clone();
        tokio::spawn(async move {
            if let Err(e) = graph.schedule_visit(&db, &record).await {
                tracing::error!(
                    visit_id = %record.visit_id,
                    "Kiosk Graph calendar scheduling failed: {e:#}"
                );
            }
        });

        calendar_scheduled = true;
    }

    tracing::info!(
        "KIOSK CHECK-IN: {} here to see {} — {}",
        visitor.name, host.name, req.reason
    );

    Ok(Json(KioskCheckInResponse {
        visit_id,
        calendar_scheduled,
        message: format!("Welcome, {}! {} has been notified.", visitor.name, host.name),
    }))
}

// ── Admin panel ───────────────────────────────────────────────

pub async fn page_admin(
    State(state): State<Arc<AppState>>,
    axum::Extension(role): axum::Extension<crate::UserRole>,
) -> Html<String> {
    apply_theme(&state);
    apply_role(&role);
    let settings = db::get_all_settings(&state.db);
    let hosts = db::list_hosts(&state.db).unwrap_or_default();
    let stats = db::get_db_stats(&state.db);
    let graph_status = if state.graph.is_some() { "connected" } else { "disabled" };
    Html(templates::admin_page(&settings, &hosts, stats, graph_status))
}

#[derive(Debug, Deserialize)]
pub struct GeneralSettingsForm {
    pub company_name: String,
    pub company_subtitle: String,
    pub receptionist_email: String,
    pub badge_expiry_text: String,
    pub timezone: String,
    pub photo_retention_hours: String,
}

pub async fn api_save_general_settings(
    State(state): State<Arc<AppState>>,
    Form(form): Form<GeneralSettingsForm>,
) -> Html<String> {
    let pairs = [
        ("company_name", form.company_name.as_str()),
        ("company_subtitle", form.company_subtitle.as_str()),
        ("receptionist_email", form.receptionist_email.as_str()),
        ("badge_expiry_text", form.badge_expiry_text.as_str()),
        ("timezone", form.timezone.as_str()),
        ("photo_retention_hours", form.photo_retention_hours.as_str()),
    ];
    for (key, val) in &pairs {
        if let Err(e) = db::set_setting(&state.db, key, val) {
            return Html(templates::alert_error(&safe_error("Failed to save settings", e)));
        }
    }
    Html(templates::alert_success("General settings saved."))
}

// ── Theme ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ThemeForm {
    pub ui_theme: String,
}

pub async fn api_save_theme(
    State(state): State<Arc<AppState>>,
    Form(form): Form<ThemeForm>,
) -> Html<String> {
    let theme = match form.ui_theme.as_str() {
        "light" | "dark" => form.ui_theme.as_str(),
        _ => "system",
    };
    if let Err(e) = db::set_setting(&state.db, "ui_theme", theme) {
        return Html(templates::alert_error(&format!("Failed: {}", e)));
    }
    // Return a script that reloads the page so the theme takes effect
    Html(format!(
        r#"<div class="alert alert-success">Theme set to {theme}.</div>
        <script>setTimeout(()=>location.reload(),500);</script>"#
    ))
}

// ── Dropdown lists ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DropdownsForm {
    pub purpose_list: String,
    pub area_list: String,
    pub visitor_type_list: String,
}

pub async fn api_save_dropdowns(
    State(state): State<Arc<AppState>>,
    Form(form): Form<DropdownsForm>,
) -> Html<String> {
    let pairs = [
        ("purpose_list", form.purpose_list.as_str()),
        ("area_list", form.area_list.as_str()),
        ("visitor_type_list", form.visitor_type_list.as_str()),
    ];
    for (key, val) in &pairs {
        if let Err(e) = db::set_setting(&state.db, key, val) {
            return Html(templates::alert_error(&format!("Failed: {}", e)));
        }
    }
    Html(templates::alert_success("Dropdown options saved."))
}

// ── Badge branding ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BadgeBrandingForm {
    pub badge_primary_color: String,
    pub badge_expiry_text: String,
    pub badge_footer_text: String,
    pub badge_type_label: String,
    pub badge_number_prefix: String,
    pub badge_label_color: String,
    pub badge_font_name_pt: String,
    pub badge_font_company_pt: String,
    pub badge_font_detail_pt: String,
    pub badge_line_spacing: String,
    #[serde(default)]
    pub badge_show_purpose: Option<String>,
    #[serde(default)]
    pub badge_show_areas: Option<String>,
    #[serde(default)]
    pub badge_show_badge_number: Option<String>,
    #[serde(default)]
    pub badge_show_escort: Option<String>,
}

pub async fn api_save_badge_branding(
    State(state): State<Arc<AppState>>,
    Form(form): Form<BadgeBrandingForm>,
) -> Html<String> {
    let pairs = [
        ("badge_primary_color", form.badge_primary_color.as_str()),
        ("badge_expiry_text", form.badge_expiry_text.as_str()),
        ("badge_footer_text", form.badge_footer_text.as_str()),
        ("badge_type_label", form.badge_type_label.as_str()),
        ("badge_number_prefix", form.badge_number_prefix.as_str()),
        ("badge_label_color", form.badge_label_color.as_str()),
        ("badge_font_name_pt", form.badge_font_name_pt.as_str()),
        ("badge_font_company_pt", form.badge_font_company_pt.as_str()),
        ("badge_font_detail_pt", form.badge_font_detail_pt.as_str()),
        ("badge_line_spacing", form.badge_line_spacing.as_str()),
        // Checkboxes: present = "1", absent = "0"
        ("badge_show_purpose", if form.badge_show_purpose.is_some() { "1" } else { "0" }),
        ("badge_show_areas", if form.badge_show_areas.is_some() { "1" } else { "0" }),
        ("badge_show_badge_number", if form.badge_show_badge_number.is_some() { "1" } else { "0" }),
        ("badge_show_escort", if form.badge_show_escort.is_some() { "1" } else { "0" }),
    ];
    for (key, val) in &pairs {
        if let Err(e) = db::set_setting(&state.db, key, val) {
            return Html(templates::alert_error(&safe_error("Failed to save settings", e)));
        }
    }
    Html(templates::alert_success("Badge branding saved."))
}

/// Upload a logo image for badge branding (PNG or JPEG, max 2MB)
pub async fn api_upload_logo(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Html<String>, StatusCode> {
    if body.is_empty() || body.len() > 2_000_000 {
        return Ok(Html(templates::alert_error(
            "Logo must be a PNG or JPEG under 2 MB.",
        )));
    }

    let filepath = state.photos_dir.join("logo.png");
    std::fs::write(&filepath, &body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let _ = db::set_setting(&state.db, "badge_logo", "logo.png");

    tracing::info!("Badge logo uploaded ({} bytes)", body.len());
    Ok(Html(templates::alert_success("Logo uploaded successfully.")))
}

/// Collected badge settings from the database (owned strings for lifetime safety)
struct BadgeSettings {
    company: String,
    expiry: String,
    color: String,
    logo: Option<String>,
    footer: String,
    badge_type: String,
    label_color: String,
    show_purpose: bool,
    show_areas: bool,
    show_badge_number: bool,
    show_escort: bool,
    font_name_pt: u8,
    font_company_pt: u8,
    font_detail_pt: u8,
    line_spacing: u8,
}

impl BadgeSettings {
    fn load(db: &db::DbPool) -> Self {
        Self {
            company: db::get_setting(db, "company_name")
                .unwrap_or_else(|| "WBBH".to_string()),
            expiry: db::get_setting(db, "badge_expiry_text")
                .unwrap_or_else(|| "VALID TODAY ONLY".to_string()),
            color: db::get_setting(db, "badge_primary_color")
                .unwrap_or_else(|| "#1a56db".to_string()),
            logo: db::get_setting(db, "badge_logo"),
            footer: db::get_setting(db, "badge_footer_text")
                .unwrap_or_default(),
            badge_type: db::get_setting(db, "badge_type_label")
                .unwrap_or_else(|| "VISITOR".to_string()),
            label_color: db::get_setting(db, "badge_label_color")
                .unwrap_or_else(|| "primary".to_string()),
            show_purpose: db::get_setting(db, "badge_show_purpose")
                .map(|v| v != "0")
                .unwrap_or(true),
            show_areas: db::get_setting(db, "badge_show_areas")
                .map(|v| v != "0")
                .unwrap_or(true),
            show_badge_number: db::get_setting(db, "badge_show_badge_number")
                .map(|v| v != "0")
                .unwrap_or(true),
            show_escort: db::get_setting(db, "badge_show_escort")
                .map(|v| v != "0")
                .unwrap_or(true),
            font_name_pt: db::get_setting(db, "badge_font_name_pt")
                .and_then(|v| v.parse().ok())
                .unwrap_or(18),
            font_company_pt: db::get_setting(db, "badge_font_company_pt")
                .and_then(|v| v.parse().ok())
                .unwrap_or(11),
            font_detail_pt: db::get_setting(db, "badge_font_detail_pt")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            line_spacing: db::get_setting(db, "badge_line_spacing")
                .and_then(|v| v.parse().ok())
                .unwrap_or(4),
        }
    }

    fn as_opts(&self) -> templates::BadgeOpts<'_> {
        templates::BadgeOpts {
            company_name: &self.company,
            expiry_text: &self.expiry,
            primary_color: &self.color,
            logo_filename: self.logo.as_deref(),
            footer_text: &self.footer,
            badge_type_label: &self.badge_type,
            badge_label_color: &self.label_color,
            show_purpose: self.show_purpose,
            show_areas: self.show_areas,
            show_badge_number: self.show_badge_number,
            show_escort: self.show_escort,
            font_name_pt: self.font_name_pt,
            font_company_pt: self.font_company_pt,
            font_detail_pt: self.font_detail_pt,
            line_spacing: self.line_spacing,
        }
    }
}

/// Badge branding preview (returns a sample badge inline)
pub async fn page_badge_preview(
    State(state): State<Arc<AppState>>,
) -> Html<String> {
    let bs = BadgeSettings::load(&state.db);

    let sample = crate::models::VisitDetail {
        id: "preview-001".to_string(),
        status: "checked_in".to_string(),
        purpose: "Meeting".to_string(),
        areas_requested: Some("Offices".to_string()),
        special_notes: Some("Requires parking pass".to_string()),
        badge_number: Some("V-001".to_string()),
        visitor_type: "Visitor".to_string(),
        pre_registered: true,
        expected_date: Some(chrono::Local::now().format("%Y-%m-%d").to_string()),
        expected_time: Some("09:00".to_string()),
        duration_minutes: Some(60),
        check_in: Some(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()),
        check_out: None,
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
        visitor: crate::models::VisitorInfo {
            id: "sample".to_string(),
            name: "First Lastname".to_string(),
            company: Some("Acme Corp".to_string()),
            phone: None,
        },
        host: crate::models::HostInfo {
            id: "sample-host".to_string(),
            name: "First Lastname".to_string(),
            department: "Department".to_string(),
            email: "john@company.com".to_string(),
            phone: None,
        },
        is_group: false,
        group_name: None,
        group_size: None,
    };

    Html(templates::badge_page_preview(&sample, None, &bs.as_opts()))
}

#[derive(Debug, Deserialize)]
pub struct SmtpSettingsForm {
    pub smtp_from_address: String,
    pub smtp_from_name: String,
}

pub async fn api_save_smtp_settings(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SmtpSettingsForm>,
) -> Html<String> {
    let pairs = [
        ("smtp_from_address", form.smtp_from_address.as_str()),
        ("smtp_from_name", form.smtp_from_name.as_str()),
    ];
    for (key, val) in &pairs {
        if let Err(e) = db::set_setting(&state.db, key, val) {
            return Html(templates::alert_error(&safe_error("Failed to save settings", e)));
        }
    }
    Html(templates::alert_success("Email settings saved."))
}

pub async fn api_test_smtp(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SmtpSettingsForm>,
) -> Html<String> {
    // Save settings first
    let pairs = [
        ("smtp_from_address", form.smtp_from_address.as_str()),
        ("smtp_from_name", form.smtp_from_name.as_str()),
    ];
    for (key, val) in &pairs {
        let _ = db::set_setting(&state.db, key, val);
    }

    let to = if form.smtp_from_address.is_empty() {
        return Html(templates::alert_error("Set a From Address first."));
    } else {
        form.smtp_from_address.clone()
    };

    match crate::email::send_test_email(&state.db, &to).await {
        Ok(_) => Html(templates::alert_success(&format!(
            "Test email sent to {} via Microsoft Graph. Check your inbox.",
            to
        ))),
        Err(e) => {
            tracing::error!("SMTP test failed: {}", e);
            Html(templates::alert_error(
                "Send failed. Make sure Graph API credentials are configured \
                 in the Calendar section above, with Mail.Send permission."
            ))
        },
    }
}

// ── Badge printing ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BadgeQuery {
    pub preview: Option<String>,
}

/// Printable badge page (opens in new tab, auto-prints unless ?preview=1)
pub async fn page_badge(
    State(state): State<Arc<AppState>>,
    Path(visit_id): Path<String>,
    Query(query): Query<BadgeQuery>,
) -> Result<Html<String>, StatusCode> {
    let detail = db::get_visit_detail(&state.db, &visit_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let bs = BadgeSettings::load(&state.db);

    // Group visits get a multi-badge page
    if detail.is_group {
        let group_size = detail.group_size.unwrap_or(1);
        let html = templates::group_badge_page(&detail, &bs.as_opts(), group_size);
        return Ok(Html(html));
    }

    let photo = db::get_visitor_photo(&state.db, &detail.visitor.id)
        .ok()
        .flatten();

    let is_preview = query.preview.as_deref() == Some("1");

    let html = if is_preview {
        templates::badge_page_preview(&detail, photo.as_deref(), &bs.as_opts())
    } else {
        templates::badge_page(&detail, photo.as_deref(), &bs.as_opts())
    };

    Ok(Html(html))
}

// ── Visit helpers ─────────────────────────────────────────────

/// Get visitor_id for a visit (used by camera JS to upload photo)
pub async fn api_visit_visitor_id(
    State(state): State<Arc<AppState>>,
    Path(visit_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let conn = state.db.lock().unwrap();
    let visitor_id: String = conn.query_row(
        "SELECT visitor_id FROM visits WHERE id = ?1",
        rusqlite::params![visit_id],
        |row| row.get(0),
    ).map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({ "visitor_id": visitor_id })))
}

// ── Photo capture ─────────────────────────────────────────────

/// Accept a JPEG photo upload for a visitor (raw body = JPEG bytes)
pub async fn api_upload_photo(
    State(state): State<Arc<AppState>>,
    Path(visitor_id): Path<String>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if body.is_empty() || body.len() > 5_000_000 {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Verify visitor exists
    let _exists = {
        let conn = state.db.lock().unwrap();
        conn.query_row(
            "SELECT id FROM visitors WHERE id = ?1",
            rusqlite::params![visitor_id],
            |row| row.get::<_, String>(0),
        ).map_err(|_| StatusCode::NOT_FOUND)?
    };

    let filename = format!("{}.jpg", visitor_id);
    let photos_dir = state.photos_dir.as_path();
    let filepath = photos_dir.join(&filename);

    std::fs::write(&filepath, &body)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    db::set_visitor_photo(&state.db, &visitor_id, &filename)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!(visitor_id = %visitor_id, "Photo saved: {}", filename);

    Ok(Json(serde_json::json!({
        "filename": filename,
        "visitor_id": visitor_id,
    })))
}

/// Serve a visitor's photo by filename
pub async fn serve_photo(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    // Prevent path traversal
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let filepath = state.photos_dir.join(&filename);
    let data = std::fs::read(&filepath).map_err(|_| StatusCode::NOT_FOUND)?;

    Ok((
        [(axum::http::header::CONTENT_TYPE, "image/jpeg")],
        data,
    ))
}

// ── Visitor / Host search (typeahead) ─────────────────────────

#[derive(Debug, Deserialize)]
pub struct TypeaheadQuery {
    pub q: Option<String>,
}

pub async fn api_search_visitors(
    State(state): State<Arc<AppState>>,
    Query(params): Query<TypeaheadQuery>,
) -> Json<Vec<serde_json::Value>> {
    let q = params.q.unwrap_or_default();
    if q.len() < 2 {
        return Json(vec![]);
    }
    let visitors = db::search_visitors(&state.db, &q).unwrap_or_default();
    let results: Vec<serde_json::Value> = visitors.iter().map(|v| {
        serde_json::json!({
            "id": v.id,
            "name": v.name,
            "company": v.company,
            "phone": v.phone,
            "email": v.email,
        })
    }).collect();
    Json(results)
}

pub async fn api_search_hosts(
    State(state): State<Arc<AppState>>,
    Query(params): Query<TypeaheadQuery>,
) -> Json<Vec<serde_json::Value>> {
    let q = params.q.unwrap_or_default();
    let hosts = db::list_hosts(&state.db).unwrap_or_default();
    if q.len() < 1 {
        // Return all hosts for empty query (shows full list)
        let results: Vec<serde_json::Value> = hosts.iter().map(|h| {
            serde_json::json!({
                "id": h.id,
                "name": h.name,
                "department": h.department,
            })
        }).collect();
        return Json(results);
    }
    let q_lower = q.to_lowercase();
    let results: Vec<serde_json::Value> = hosts.iter()
        .filter(|h| h.name.to_lowercase().contains(&q_lower)
                  || h.department.to_lowercase().contains(&q_lower))
        .map(|h| {
            serde_json::json!({
                "id": h.id,
                "name": h.name,
                "department": h.department,
            })
        }).collect();
    Json(results)
}

// ── Utilities ─────────────────────────────────────────────────

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.trim().is_empty())
}

fn render_full_table(visits: &[VisitDetail], show_actions: bool) -> String {
    if visits.is_empty() {
        return "<p style='color:var(--text-dim);padding:1rem;'>\
                No matching visits.</p>"
            .to_string();
    }
    let action_header = if show_actions { "<th>Actions</th>" } else { "" };
    let rows: String = visits
        .iter()
        .map(|v| templates::visit_row_partial(v))
        .collect();
    format!(
        r#"<table>
        <thead><tr>
            <th>Visitor</th><th>Company</th><th>Host</th>
            <th>Purpose</th><th>Expected</th><th>Status</th>
            <th>In</th><th>Out</th>
            {action_header}
        </tr></thead>
        <tbody>{rows}</tbody>
    </table>"#
    )
}
