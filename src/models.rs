use serde::{Deserialize, Serialize};

// ── Host ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Host {
    pub id: String,
    pub name: String,
    pub department: String,
    pub email: String,
    pub phone: Option<String>,
    pub active: bool,
}

#[derive(Debug, Deserialize)]
pub struct NewHost {
    pub name: String,
    pub department: String,
    pub email: String,
    pub phone: Option<String>,
}

// ── Visitor ───────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Visitor {
    pub id: String,
    pub name: String,
    pub company: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewVisitor {
    pub name: String,
    pub company: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub notes: Option<String>,
}

// ── Visit ─────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Visit {
    pub id: String,
    pub visitor_id: String,
    pub host_id: String,
    pub purpose: String,
    pub areas_requested: Option<String>,
    pub badge_number: Option<String>,
    pub status: String,
    pub pre_registered: bool,
    pub expected_date: Option<String>,
    pub check_in: Option<String>,
    pub check_out: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NewVisit {
    pub visitor_id: String,
    pub host_id: String,
    pub purpose: String,
    pub areas_requested: Option<String>,
    pub special_notes: Option<String>,
    pub visitor_type: String,
    pub status: String,
    pub pre_registered: bool,
    pub expected_date: Option<String>,
    pub expected_time: Option<String>,
    pub duration_minutes: Option<i32>,
}

// ── Joined view for display ───────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct VisitDetail {
    pub id: String,
    pub status: String,
    pub purpose: String,
    pub areas_requested: Option<String>,
    pub special_notes: Option<String>,
    pub badge_number: Option<String>,
    pub visitor_type: String,
    pub pre_registered: bool,
    pub expected_date: Option<String>,
    pub expected_time: Option<String>,
    pub duration_minutes: Option<i32>,
    pub check_in: Option<String>,
    pub check_out: Option<String>,
    pub created_at: String,
    pub visitor: VisitorInfo,
    pub host: HostInfo,
}

#[derive(Debug, Serialize, Clone)]
pub struct VisitorInfo {
    pub id: String,
    pub name: String,
    pub company: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct HostInfo {
    pub id: String,
    pub name: String,
    pub department: String,
    pub email: String,
    pub phone: Option<String>,
}

// ── Form input structs ────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PreRegisterForm {
    pub visitor_name: String,
    pub visitor_company: Option<String>,
    pub visitor_phone: Option<String>,
    pub visitor_email: Option<String>,
    pub host_id: String,
    pub purpose: String,
    pub visitor_type: Option<String>,
    pub areas_requested: Option<String>,
    pub expected_date: String,
    pub expected_time: Option<String>,
    pub duration: Option<String>,
    pub special_notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WalkInForm {
    pub visitor_name: String,
    pub visitor_company: Option<String>,
    pub visitor_phone: Option<String>,
    pub host_id: String,
    pub purpose: String,
    pub visitor_type: Option<String>,
    pub areas_requested: Option<String>,
    pub special_notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}
