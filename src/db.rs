use rusqlite::{Connection, Result, params};
use std::sync::{Arc, Mutex};
use crate::models::*;

pub type DbPool = Arc<Mutex<Connection>>;

pub fn init_db(path: &str) -> Result<DbPool> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    run_migrations(&conn)?;
    Ok(Arc::new(Mutex::new(conn)))
}

fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS hosts (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            department  TEXT NOT NULL,
            email       TEXT NOT NULL,
            phone       TEXT,
            active      INTEGER NOT NULL DEFAULT 1,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS visitors (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            company     TEXT,
            phone       TEXT,
            email       TEXT,
            notes       TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS visits (
            id              TEXT PRIMARY KEY,
            visitor_id      TEXT NOT NULL REFERENCES visitors(id),
            host_id         TEXT NOT NULL REFERENCES hosts(id),
            purpose         TEXT NOT NULL,
            areas_requested TEXT,
            badge_number    TEXT,
            status          TEXT NOT NULL DEFAULT 'pending',
            pre_registered  INTEGER NOT NULL DEFAULT 0,
            expected_date   TEXT,
            check_in        TEXT,
            check_out       TEXT,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_visits_status ON visits(status);
        CREATE INDEX IF NOT EXISTS idx_visits_date ON visits(expected_date);
        CREATE INDEX IF NOT EXISTS idx_visits_host ON visits(host_id);
        CREATE INDEX IF NOT EXISTS idx_visitors_name ON visitors(name);
        "
    )?;

    // Graph integration columns (idempotent — safe on existing DBs)
    let graph_columns = [
        ("graph_event_id", "TEXT"),
        ("graph_web_link", "TEXT"),
        ("calendar_status", "TEXT DEFAULT 'none'"),
    ];
    for (col, col_type) in &graph_columns {
        let sql = format!("ALTER TABLE visits ADD COLUMN {col} {col_type}");
        let _ = conn.execute(&sql, []);
    }

    // Time/duration columns on visits
    let _ = conn.execute(
        "ALTER TABLE visits ADD COLUMN expected_time TEXT", [],
    );
    let _ = conn.execute(
        "ALTER TABLE visits ADD COLUMN duration_minutes INTEGER", [],
    );

    // Photo column on visitors (stores filename, not full path)
    let _ = conn.execute(
        "ALTER TABLE visitors ADD COLUMN photo_filename TEXT",
        [],
    );

    // Special notes on visits
    let _ = conn.execute(
        "ALTER TABLE visits ADD COLUMN special_notes TEXT",
        [],
    );

    // Visitor type per visit (VISITOR, GUEST, CONTRACTOR, etc.)
    let _ = conn.execute(
        "ALTER TABLE visits ADD COLUMN visitor_type TEXT DEFAULT 'Visitor'",
        [],
    );

    // Group visit columns
    let _ = conn.execute("ALTER TABLE visits ADD COLUMN is_group INTEGER DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE visits ADD COLUMN group_name TEXT", []);
    let _ = conn.execute("ALTER TABLE visits ADD COLUMN group_size INTEGER", []);

    // Sessions table for auth
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (
            token       TEXT PRIMARY KEY,
            role        TEXT NOT NULL DEFAULT 'user',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            expires_at  TEXT NOT NULL
        );"
    )?;

    // Migration: add role column if missing (for existing V2 databases)
    let _ = conn.execute("ALTER TABLE sessions ADD COLUMN role TEXT NOT NULL DEFAULT 'user'", []);

    // V2: Remove graph secrets from settings (now env-var only)
    let _ = conn.execute_batch(
        "DELETE FROM settings WHERE key IN (
            'graph_tenant_id', 'graph_client_id', 'graph_client_secret',
            'graph_group_id', 'graph_group_email'
        );"
    );

    // Settings table (key-value, admin-configurable)
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );"
    )?;

    // TOTP backup codes — argon2-hashed, single-use. Generated 10 at a
    // time alongside the TOTP secret. Used only as a recovery path when
    // the admin's authenticator is lost.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS totp_backup_codes (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            code_hash  TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            used_at    TEXT
        );"
    )?;

    // Seed defaults (only if not already set)
    let defaults = [
        ("company_name", "Your Company"),
        ("company_subtitle", "Visitor Management"),
        ("timezone", "Eastern Standard Time"),
        ("receptionist_email", ""),
        ("badge_expiry_text", "VALID TODAY ONLY"),
        ("smtp_host", ""),
        ("smtp_port", "587"),
        ("smtp_username", ""),
        ("smtp_password", ""),
        ("smtp_from_address", ""),
        ("smtp_from_name", "GateKeeper"),
        ("badge_primary_color", "#1a56db"),
        ("badge_logo", ""),
        ("badge_footer_text", ""),
        ("badge_type_label", "VISITOR"),
        ("badge_number_prefix", "V-"),
        ("badge_label_color", "primary"),
        ("badge_show_purpose", "1"),
        ("badge_show_areas", "1"),
        ("badge_show_badge_number", "1"),
        ("badge_show_escort", "1"),
        ("photo_retention_hours", "24"),
        ("visit_retention_hours", "8"),
        ("visitor_type_list", "Visitor,Guest,Contractor,Vendor,Interview"),
        ("badge_font_name_pt", "18"),
        ("badge_font_company_pt", "11"),
        ("badge_font_detail_pt", "10"),
        ("badge_line_spacing", "4"),
    ];
    for (key, val) in &defaults {
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, val],
        )?;
    }

    Ok(())
}

// ── Session queries ──────────────────────────────────────────

pub fn create_session(db: &DbPool, token: &str, role: &str, expires_hours: i64) -> Result<()> {
    let conn = db.lock().unwrap();
    let now = chrono::Local::now();
    let expires = now + chrono::Duration::hours(expires_hours);
    conn.execute(
        "INSERT INTO sessions (token, role, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        params![
            token,
            role,
            now.format("%Y-%m-%d %H:%M:%S").to_string(),
            expires.format("%Y-%m-%d %H:%M:%S").to_string(),
        ],
    )?;
    Ok(())
}

/// Returns the role ("admin" or "user") if the session is valid, None otherwise.
pub fn validate_session(db: &DbPool, token: &str) -> Option<String> {
    let conn = db.lock().unwrap();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.query_row(
        "SELECT role FROM sessions WHERE token = ?1 AND expires_at > ?2",
        params![token, now],
        |row| row.get::<_, String>(0),
    ).ok()
}

pub fn delete_session(db: &DbPool, token: &str) {
    let conn = db.lock().unwrap();
    let _ = conn.execute("DELETE FROM sessions WHERE token = ?1", params![token]);
}

pub fn cleanup_expired_sessions(db: &DbPool) {
    let conn = db.lock().unwrap();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let _ = conn.execute("DELETE FROM sessions WHERE expires_at < ?1", params![now]);
}

// ── Settings queries ──────────────────────────────────────────

pub fn get_setting(db: &DbPool, key: &str) -> Option<String> {
    let conn = db.lock().unwrap();
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    ).ok()
}

pub fn get_all_settings(db: &DbPool) -> Vec<(String, String)> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare("SELECT key, value FROM settings ORDER BY key")
        .unwrap();
    stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .unwrap()
    .filter_map(|r| r.ok())
    .collect()
}

pub fn set_setting(db: &DbPool, key: &str, value: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = ?2",
        params![key, value],
    )?;
    Ok(())
}

/// Check if a TOTP secret has been configured for admin MFA.
pub fn has_totp_secret(db: &DbPool) -> bool {
    get_setting(db, "totp_secret")
        .map(|s| !s.is_empty())
        .unwrap_or(false)
}

/// Generate the next badge number for today. Counts today's checked-in visits
/// and returns prefix + zero-padded sequence (e.g., "V-001", "V-002").
pub fn next_badge_number(db: &DbPool) -> String {
    let conn = db.lock().unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM visits WHERE status = 'checked_in' AND check_in LIKE ?1 || '%'",
        params![today],
        |r| r.get(0),
    ).unwrap_or(0);
    let prefix = conn.query_row(
        "SELECT value FROM settings WHERE key = 'badge_number_prefix'",
        [],
        |r| r.get::<_, String>(0),
    ).unwrap_or_else(|_| "V-".to_string());
    format!("{}{:03}", prefix, count + 1)
}

/// Returns photo filenames for visitors whose most recent visit ended more than
/// `retention_hours` ago. Excludes "logo.png".
pub fn expired_photo_filenames(db: &DbPool, retention_hours: i64) -> Vec<String> {
    let conn = db.lock().unwrap();
    let cutoff = (chrono::Local::now() - chrono::Duration::hours(retention_hours))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT v.photo_filename
         FROM visitors v
         JOIN visits vt ON vt.visitor_id = v.id
         WHERE v.photo_filename IS NOT NULL
           AND v.photo_filename != ''
           AND v.photo_filename != 'logo.png'
           AND NOT EXISTS (
               SELECT 1 FROM visits vt2
               WHERE vt2.visitor_id = v.id
                 AND (vt2.status IN ('pending', 'checked_in')
                      OR vt2.updated_at > ?1)
           )"
    ).unwrap();
    stmt.query_map(params![cutoff], |row| row.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
}

/// Clear the photo_filename field for visitors whose photos were cleaned up.
pub fn clear_expired_photos(db: &DbPool, retention_hours: i64) -> usize {
    let conn = db.lock().unwrap();
    let cutoff = (chrono::Local::now() - chrono::Duration::hours(retention_hours))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    conn.execute(
        "UPDATE visitors SET photo_filename = NULL
         WHERE photo_filename IS NOT NULL
           AND photo_filename != ''
           AND photo_filename != 'logo.png'
           AND NOT EXISTS (
               SELECT 1 FROM visits vt
               WHERE vt.visitor_id = visitors.id
                 AND (vt.status IN ('pending', 'checked_in')
                      OR vt.updated_at > ?1)
           )",
        params![cutoff],
    ).unwrap_or(0)
}

/// Result of a retention sweep over checked-out visits. `photo_filenames`
/// are paths that must be unlinked from disk by the caller (the DB no
/// longer references them after this call).
#[derive(Debug, Default)]
pub struct PurgeResult {
    pub visits_deleted: usize,
    pub visitors_deleted: usize,
    pub photo_filenames: Vec<String>,
}

/// Purge checked-out visits older than `hours`, plus any visitor rows that
/// become orphaned (no remaining visit with status != 'checked_out' or
/// with check_out within the retention window). This implements Path A
/// minimization for the §10 PII-locality compliance posture: visitor PII
/// (name, phone, email, photo) is removed once the visit is settled and
/// the retention window has elapsed. The host's M365 calendar event
/// remains as the canonical long-term audit trail.
pub fn purge_old_visits(db: &DbPool, hours: i64) -> PurgeResult {
    if hours <= 0 {
        return PurgeResult::default();
    }
    let conn = db.lock().unwrap();
    let cutoff = (chrono::Local::now() - chrono::Duration::hours(hours))
        .format("%Y-%m-%d %H:%M")
        .to_string();

    // Identify visitors who would have no retained visits after this sweep.
    // A visit is "retained" if its status is anything other than
    // checked_out, OR if it's checked_out but within the retention window.
    let orphan_visitors: Vec<(String, Option<String>)> = {
        let mut stmt = match conn.prepare(
            "SELECT v.id, v.photo_filename
             FROM visitors v
             WHERE NOT EXISTS (
                 SELECT 1 FROM visits vt
                 WHERE vt.visitor_id = v.id
                   AND (vt.status != 'checked_out'
                        OR vt.check_out IS NULL
                        OR vt.check_out >= ?1)
             )",
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("retention sweep: prepare failed: {}", e);
                return PurgeResult::default();
            }
        };
        stmt.query_map(params![cutoff], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    // Step 1: delete the old checked-out visits.
    let visits_deleted = conn
        .execute(
            "DELETE FROM visits
             WHERE status = 'checked_out'
               AND check_out IS NOT NULL
               AND check_out < ?1",
            params![cutoff],
        )
        .unwrap_or_else(|e| {
            tracing::error!("retention sweep: visits delete failed: {}", e);
            0
        });

    // Step 2: delete the orphaned visitor rows.
    let mut visitors_deleted = 0usize;
    for (id, _) in &orphan_visitors {
        match conn.execute("DELETE FROM visitors WHERE id = ?1", params![id]) {
            Ok(n) => visitors_deleted += n,
            Err(e) => {
                tracing::error!(
                    "retention sweep: visitor {} delete failed: {}",
                    id,
                    e
                );
            }
        }
    }

    // Step 3: collect photo filenames so the caller can unlink them.
    let photo_filenames = orphan_visitors
        .into_iter()
        .filter_map(|(_, photo)| photo)
        .filter(|f| !f.is_empty() && f != "logo.png")
        .collect();

    PurgeResult {
        visits_deleted,
        visitors_deleted,
        photo_filenames,
    }
}

// ── TOTP backup codes ────────────────────────────────────────

fn normalize_backup_code(presented: &str) -> String {
    presented
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Generate, hash, and store 10 fresh backup codes. Wipes any previously
/// issued codes so each enrollment replaces the prior batch. Returns the
/// 10 plaintext codes (formatted xxxx-xxxx) for one-time display to the
/// admin — they are not retrievable after this call.
pub fn rotate_backup_codes(db: &DbPool) -> Result<Vec<String>> {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    let conn = db.lock().unwrap();
    conn.execute("DELETE FROM totp_backup_codes", [])?;
    let argon = argon2::Argon2::default();
    let mut codes = Vec::with_capacity(10);
    for _ in 0..10 {
        let raw = uuid::Uuid::new_v4().simple().to_string();
        let display_code = format!("{}-{}", &raw[..4], &raw[4..8]);
        let normalized = normalize_backup_code(&display_code);
        let salt = SaltString::generate(&mut OsRng);
        let hash = argon
            .hash_password(normalized.as_bytes(), &salt)
            .map_err(|e| {
                rusqlite::Error::ToSqlConversionFailure(Box::new(
                    std::io::Error::other(e.to_string()),
                ))
            })?
            .to_string();
        conn.execute(
            "INSERT INTO totp_backup_codes (code_hash) VALUES (?1)",
            params![hash],
        )?;
        codes.push(display_code);
    }
    Ok(codes)
}

/// Verify a presented backup code (dashes/whitespace/case-insensitive) and
/// mark it consumed if it matches an unused row. Returns true on match.
pub fn consume_backup_code(db: &DbPool, presented: &str) -> bool {
    use argon2::{Argon2, PasswordHash, PasswordVerifier};
    let normalized = normalize_backup_code(presented);
    if normalized.is_empty() {
        return false;
    }

    let conn = db.lock().unwrap();
    let mut stmt = match conn.prepare(
        "SELECT id, code_hash FROM totp_backup_codes WHERE used_at IS NULL",
    ) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let rows: Vec<(i64, String)> =
        match stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?))) {
            Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
            Err(_) => return false,
        };
    drop(stmt);

    let argon = Argon2::default();
    for (id, hash) in &rows {
        let parsed = match PasswordHash::new(hash) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if argon.verify_password(normalized.as_bytes(), &parsed).is_ok() {
            let _ = conn.execute(
                "UPDATE totp_backup_codes SET used_at = datetime('now') WHERE id = ?1",
                params![id],
            );
            return true;
        }
    }
    false
}

/// Count remaining unused backup codes — for "X codes left" warnings.
pub fn backup_codes_remaining(db: &DbPool) -> usize {
    let conn = db.lock().unwrap();
    conn.query_row(
        "SELECT COUNT(*) FROM totp_backup_codes WHERE used_at IS NULL",
        [],
        |r| r.get::<_, i64>(0).map(|n| n as usize),
    )
    .unwrap_or(0)
}

pub fn get_db_stats(db: &DbPool) -> (usize, usize, usize) {
    let conn = db.lock().unwrap();
    let hosts: usize = conn.query_row(
        "SELECT COUNT(*) FROM hosts WHERE active = 1", [], |r| r.get(0)
    ).unwrap_or(0);
    let visitors: usize = conn.query_row(
        "SELECT COUNT(*) FROM visitors", [], |r| r.get(0)
    ).unwrap_or(0);
    let visits: usize = conn.query_row(
        "SELECT COUNT(*) FROM visits", [], |r| r.get(0)
    ).unwrap_or(0);
    (hosts, visitors, visits)
}

// ── Host queries ──────────────────────────────────────────────

pub fn list_hosts(db: &DbPool) -> Result<Vec<Host>> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, name, department, email, phone, active FROM hosts WHERE active = 1 ORDER BY name"
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Host {
            id: row.get(0)?,
            name: row.get(1)?,
            department: row.get(2)?,
            email: row.get(3)?,
            phone: row.get(4)?,
            active: row.get(5)?,
        })
    })?;
    rows.collect()
}

pub fn insert_host(db: &DbPool, host: &NewHost) -> Result<String> {
    let conn = db.lock().unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO hosts (id, name, department, email, phone) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, host.name, host.department, host.email, host.phone],
    )?;
    Ok(id)
}

pub fn update_host(db: &DbPool, id: &str, host: &NewHost) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "UPDATE hosts SET name = ?1, department = ?2, email = ?3, phone = ?4 WHERE id = ?5",
        params![host.name, host.department, host.email, host.phone, id],
    )?;
    Ok(())
}

pub fn deactivate_host(db: &DbPool, id: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "UPDATE hosts SET active = 0 WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn get_host(db: &DbPool, id: &str) -> Result<Option<Host>> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, name, department, email, phone, active FROM hosts WHERE id = ?1"
    )?;
    let mut rows = stmt.query_map(params![id], |row| {
        Ok(Host {
            id: row.get(0)?,
            name: row.get(1)?,
            department: row.get(2)?,
            email: row.get(3)?,
            phone: row.get(4)?,
            active: row.get(5)?,
        })
    })?;
    match rows.next() {
        Some(Ok(h)) => Ok(Some(h)),
        _ => Ok(None),
    }
}

// ── Visitor queries ───────────────────────────────────────────

pub fn search_visitors(db: &DbPool, query: &str) -> Result<Vec<Visitor>> {
    let conn = db.lock().unwrap();
    let pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT id, name, company, phone, email, notes FROM visitors
         WHERE name LIKE ?1 OR company LIKE ?1
         ORDER BY name LIMIT 10"
    )?;
    let rows = stmt.query_map(params![pattern], |row| {
        Ok(Visitor {
            id: row.get(0)?,
            name: row.get(1)?,
            company: row.get(2)?,
            phone: row.get(3)?,
            email: row.get(4)?,
            notes: row.get(5)?,
        })
    })?;
    rows.collect()
}

pub fn find_or_create_visitor(db: &DbPool, visitor: &NewVisitor) -> Result<String> {
    let conn = db.lock().unwrap();
    // Try to find existing visitor by name + company
    let mut stmt = conn.prepare(
        "SELECT id FROM visitors WHERE lower(name) = lower(?1) AND (company = ?2 OR (?2 IS NULL AND company IS NULL)) LIMIT 1"
    )?;
    let existing: Option<String> = stmt.query_row(
        params![visitor.name, visitor.company],
        |row| row.get(0),
    ).ok();

    if let Some(id) = existing {
        return Ok(id);
    }

    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO visitors (id, name, company, phone, email, notes) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, visitor.name, visitor.company, visitor.phone, visitor.email, visitor.notes],
    )?;
    Ok(id)
}

pub fn set_visitor_photo(db: &DbPool, visitor_id: &str, filename: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    conn.execute(
        "UPDATE visitors SET photo_filename = ?1 WHERE id = ?2",
        params![filename, visitor_id],
    )?;
    Ok(())
}

pub fn get_visitor_photo(db: &DbPool, visitor_id: &str) -> Result<Option<String>> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT photo_filename FROM visitors WHERE id = ?1"
    )?;
    let result: Option<String> = stmt.query_row(params![visitor_id], |row| row.get(0)).ok();
    Ok(result)
}

// ── Visit queries ─────────────────────────────────────────────

pub fn create_visit(db: &DbPool, visit: &NewVisit) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT INTO visits (id, visitor_id, host_id, purpose, areas_requested, special_notes, visitor_type, status, pre_registered, expected_date, expected_time, duration_minutes, is_group, group_name, group_size, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)",
        params![
            id,
            visit.visitor_id,
            visit.host_id,
            visit.purpose,
            visit.areas_requested,
            visit.special_notes,
            visit.visitor_type,
            visit.status,
            visit.pre_registered,
            visit.expected_date,
            visit.expected_time,
            visit.duration_minutes,
            visit.is_group,
            visit.group_name,
            visit.group_size,
            now,
        ],
    )?;
    Ok(id)
}

pub fn check_in_visit(db: &DbPool, visit_id: &str, badge_number: Option<&str>) -> Result<()> {
    let conn = db.lock().unwrap();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "UPDATE visits SET status = 'checked_in', check_in = ?1, badge_number = ?2, updated_at = ?1 WHERE id = ?3",
        params![now, badge_number, visit_id],
    )?;
    Ok(())
}

/// Promote rescheduled visits to pending when their expected date arrives
pub fn promote_rescheduled_visits(db: &DbPool) -> usize {
    let conn = db.lock().unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    conn.execute(
        "UPDATE visits SET status = 'pending' WHERE status = 'rescheduled' AND date(expected_date) <= ?1",
        params![today],
    )
    .unwrap_or(0)
}

pub fn check_out_all_today(db: &DbPool) -> Result<usize> {
    let conn = db.lock().unwrap();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let count = conn.execute(
        "UPDATE visits SET status = 'checked_out', check_out = ?1, updated_at = ?1
         WHERE status = 'checked_in' AND date(check_in) = ?2",
        params![now, today],
    )?;
    Ok(count)
}

pub fn check_out_visit(db: &DbPool, visit_id: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "UPDATE visits SET status = 'checked_out', check_out = ?1, updated_at = ?1 WHERE id = ?2",
        params![now, visit_id],
    )?;
    Ok(())
}

pub fn update_visit_status(db: &DbPool, visit_id: &str, status: &str) -> Result<()> {
    let conn = db.lock().unwrap();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    conn.execute(
        "UPDATE visits SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status, now, visit_id],
    )?;
    Ok(())
}

/// Push the expected_time forward by N minutes. If no time is set, uses now.
pub fn push_expected_time(db: &DbPool, visit_id: &str, delay_minutes: i32) -> Result<()> {
    let conn = db.lock().unwrap();
    let current_time: Option<String> = conn.query_row(
        "SELECT expected_time FROM visits WHERE id = ?1",
        params![visit_id],
        |row| row.get(0),
    )?;

    let base = match current_time.as_deref() {
        Some(t) if !t.is_empty() => {
            let parts: Vec<&str> = t.split(':').collect();
            let h: u32 = parts.first().and_then(|p| p.parse().ok()).unwrap_or(9);
            let m: u32 = parts.get(1).and_then(|p| p.parse().ok()).unwrap_or(0);
            chrono::NaiveTime::from_hms_opt(h, m, 0)
                .unwrap_or_else(|| chrono::Local::now().time())
        }
        _ => chrono::Local::now().time(),
    };

    let new_time = base + chrono::Duration::minutes(delay_minutes as i64);
    let new_time_str = new_time.format("%H:%M").to_string();

    conn.execute(
        "UPDATE visits SET expected_time = ?1 WHERE id = ?2",
        params![new_time_str, visit_id],
    )?;
    Ok(())
}


/// Reschedule a visit to a new date and optionally a new time
pub fn reschedule_visit(
    db: &DbPool,
    visit_id: &str,
    new_date: &str,
    new_time: Option<&str>,
) -> Result<()> {
    let conn = db.lock().unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    // If rescheduled to today, reset to pending so it shows on the dashboard
    let new_status = if new_date == today { "pending" } else { "rescheduled" };
    conn.execute(
        "UPDATE visits SET expected_date = ?1, expected_time = ?2, status = ?3 WHERE id = ?4",
        params![new_date, new_time, new_status, visit_id],
    )?;
    Ok(())
}

/// Shared row mapper for the standard visit+visitor+host SELECT
fn visit_detail_from_row(row: &rusqlite::Row) -> Result<VisitDetail> {
    Ok(VisitDetail {
        id: row.get(0)?,
        status: row.get(1)?,
        purpose: row.get(2)?,
        areas_requested: row.get(3)?,
        special_notes: row.get(4)?,
        badge_number: row.get(5)?,
        visitor_type: row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "Visitor".to_string()),
        pre_registered: row.get(7)?,
        expected_date: row.get(8)?,
        expected_time: row.get(9)?,
        duration_minutes: row.get(10)?,
        check_in: row.get(11)?,
        check_out: row.get(12)?,
        created_at: row.get(13)?,
        visitor: VisitorInfo {
            id: row.get(14)?,
            name: row.get(15)?,
            company: row.get(16)?,
            phone: row.get(17)?,
        },
        host: HostInfo {
            id: row.get(18)?,
            name: row.get(19)?,
            department: row.get(20)?,
            email: row.get(21)?,
            phone: row.get(22)?,
        },
        is_group: row.get::<_, Option<i64>>(23)?.unwrap_or(0) != 0,
        group_name: row.get(24)?,
        group_size: row.get(25)?,
    })
}

pub fn get_visit_detail(db: &DbPool, visit_id: &str) -> Result<Option<VisitDetail>> {
    let conn = db.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT v.id, v.status, v.purpose, v.areas_requested, v.special_notes, v.badge_number,
                v.visitor_type, v.pre_registered, v.expected_date, v.expected_time, v.duration_minutes,
                v.check_in, v.check_out, v.created_at,
                vis.id, vis.name, vis.company, vis.phone,
                h.id, h.name, h.department, h.email, h.phone,
                v.is_group, v.group_name, v.group_size
         FROM visits v
         JOIN visitors vis ON v.visitor_id = vis.id
         JOIN hosts h ON v.host_id = h.id
         WHERE v.id = ?1"
    )?;
    let mut rows = stmt.query_map(params![visit_id], |row| {
        Ok(visit_detail_from_row(row)?)
    })?;
    match rows.next() {
        Some(Ok(detail)) => Ok(Some(detail)),
        _ => Ok(None),
    }
}

pub fn list_visits_today(db: &DbPool) -> Result<Vec<VisitDetail>> {
    let conn = db.lock().unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut stmt = conn.prepare(
        "SELECT v.id, v.status, v.purpose, v.areas_requested, v.special_notes, v.badge_number,
                v.visitor_type, v.pre_registered, v.expected_date, v.expected_time, v.duration_minutes,
                v.check_in, v.check_out, v.created_at,
                vis.id, vis.name, vis.company, vis.phone,
                h.id, h.name, h.department, h.email, h.phone,
                v.is_group, v.group_name, v.group_size
         FROM visits v
         JOIN visitors vis ON v.visitor_id = vis.id
         JOIN hosts h ON v.host_id = h.id
         WHERE (date(v.expected_date) = ?1 OR date(v.check_in) = ?1
                OR (v.status IN ('pending','checked_in','approved','running_late')
                    AND (date(v.created_at) = ?1 OR date(v.created_at, 'localtime') = ?1)))
           AND NOT (v.status = 'rescheduled' AND date(v.expected_date) > ?1)
         ORDER BY
            CASE v.status
                WHEN 'pending' THEN 0
                WHEN 'running_late' THEN 1
                WHEN 'approved' THEN 2
                WHEN 'checked_in' THEN 3
                WHEN 'checked_out' THEN 4
                WHEN 'denied' THEN 5
                WHEN 'rescheduled' THEN 6
                ELSE 7
            END,
            v.expected_time ASC NULLS LAST,
            v.created_at DESC"
    )?;
    let rows = stmt.query_map(params![today], |row| {
        Ok(visit_detail_from_row(row)?)
    })?;
    rows.collect()
}

pub fn list_preregistered_upcoming(db: &DbPool) -> Result<Vec<VisitDetail>> {
    let conn = db.lock().unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut stmt = conn.prepare(
        "SELECT v.id, v.status, v.purpose, v.areas_requested, v.special_notes, v.badge_number,
                v.visitor_type, v.pre_registered, v.expected_date, v.expected_time, v.duration_minutes,
                v.check_in, v.check_out, v.created_at,
                vis.id, vis.name, vis.company, vis.phone,
                h.id, h.name, h.department, h.email, h.phone,
                v.is_group, v.group_name, v.group_size
         FROM visits v
         JOIN visitors vis ON v.visitor_id = vis.id
         JOIN hosts h ON v.host_id = h.id
         WHERE v.status IN ('pending', 'rescheduled') AND date(v.expected_date) > ?1
         ORDER BY v.expected_date ASC, v.expected_time ASC NULLS LAST"
    )?;
    let rows = stmt.query_map(params![today], |row| {
        Ok(visit_detail_from_row(row)?)
    })?;
    rows.collect()
}

pub fn search_visits(db: &DbPool, query: &str, from: Option<&str>, to: Option<&str>) -> Result<Vec<VisitDetail>> {
    let conn = db.lock().unwrap();
    let search = format!("%{}%", query.to_lowercase());
    let from_date = from.unwrap_or("2000-01-01");
    let to_date = to.unwrap_or("2099-12-31");
    let mut stmt = conn.prepare(
        "SELECT v.id, v.status, v.purpose, v.areas_requested, v.special_notes, v.badge_number,
                v.visitor_type, v.pre_registered, v.expected_date, v.expected_time, v.duration_minutes,
                v.check_in, v.check_out, v.created_at,
                vis.id, vis.name, vis.company, vis.phone,
                h.id, h.name, h.department, h.email, h.phone,
                v.is_group, v.group_name, v.group_size
         FROM visits v
         JOIN visitors vis ON v.visitor_id = vis.id
         JOIN hosts h ON v.host_id = h.id
         WHERE (lower(vis.name) LIKE ?1 OR lower(vis.company) LIKE ?1 OR lower(h.name) LIKE ?1 OR lower(v.purpose) LIKE ?1)
           AND date(v.created_at) BETWEEN ?2 AND ?3
         ORDER BY v.created_at DESC
         LIMIT 100"
    )?;
    let rows = stmt.query_map(params![search, from_date, to_date], |row| {
        Ok(visit_detail_from_row(row)?)
    })?;
    rows.collect()
}
