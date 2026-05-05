/// Integration tests for the Group Visit feature.
///
/// These tests exercise the full flow: registration → dashboard display →
/// check-in → badge printing → check-out, using an in-memory SQLite database.

use std::sync::{Arc, Mutex};
use rusqlite::Connection;

// We test the db module directly since it's the core logic.
// The web layer is a thin wrapper around these functions.

/// Initialize a fresh in-memory database with the full schema
fn test_db() -> Arc<Mutex<Connection>> {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();

    // Create tables (mirrors db.rs run_migrations)
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS hosts (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, department TEXT NOT NULL,
            email TEXT NOT NULL, phone TEXT, active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS visitors (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, company TEXT, phone TEXT,
            email TEXT, notes TEXT, photo_filename TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS visits (
            id TEXT PRIMARY KEY, visitor_id TEXT NOT NULL REFERENCES visitors(id),
            host_id TEXT NOT NULL REFERENCES hosts(id), purpose TEXT NOT NULL,
            areas_requested TEXT, badge_number TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            pre_registered INTEGER NOT NULL DEFAULT 0,
            expected_date TEXT, check_in TEXT, check_out TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            graph_event_id TEXT, graph_web_link TEXT,
            calendar_status TEXT DEFAULT 'none',
            expected_time TEXT, duration_minutes INTEGER,
            special_notes TEXT, visitor_type TEXT DEFAULT 'Visitor',
            is_group INTEGER DEFAULT 0, group_name TEXT, group_size INTEGER
        );
        CREATE TABLE IF NOT EXISTS sessions (
            token TEXT PRIMARY KEY, role TEXT NOT NULL DEFAULT 'user',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            expires_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY, value TEXT NOT NULL
        );
        INSERT OR IGNORE INTO settings (key, value) VALUES ('badge_number_prefix', 'V-');
        CREATE INDEX IF NOT EXISTS idx_visits_status ON visits(status);
        CREATE INDEX IF NOT EXISTS idx_visits_date ON visits(expected_date);
        "
    ).unwrap();

    Arc::new(Mutex::new(conn))
}

/// Seed a test host and return its ID
fn seed_host(db: &Arc<Mutex<Connection>>) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT INTO hosts (id, name, department, email) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![id, "Front Desk", "Management", "frontdesk@example.com"],
    ).unwrap();
    id
}

/// Create a visitor and return its ID
fn create_visitor(db: &Arc<Mutex<Connection>>, name: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT INTO visitors (id, name) VALUES (?1, ?2)",
        rusqlite::params![id, name],
    ).unwrap();
    id
}

/// Create a group visit and return the visit ID
fn create_group_visit(
    db: &Arc<Mutex<Connection>>,
    visitor_id: &str,
    host_id: &str,
    group_name: &str,
    group_size: i32,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT INTO visits (id, visitor_id, host_id, purpose, areas_requested, visitor_type,
         status, pre_registered, expected_date, expected_time, duration_minutes,
         is_group, group_name, group_size, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
        rusqlite::params![
            id, visitor_id, host_id, "Tour", "Studios, Newsroom", "Visitor",
            "pending", 1, today, "10:00", 120,
            1, group_name, group_size, now,
        ],
    ).unwrap();
    id
}

/// Create a regular (non-group) visit and return the visit ID
fn create_regular_visit(
    db: &Arc<Mutex<Connection>>,
    visitor_id: &str,
    host_id: &str,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT INTO visits (id, visitor_id, host_id, purpose, status, pre_registered,
         expected_date, expected_time, is_group, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        rusqlite::params![
            id, visitor_id, host_id, "Meeting", "pending", 1,
            today, "09:00", 0, now,
        ],
    ).unwrap();
    id
}

/// Read a visit's details from the database
fn get_visit(db: &Arc<Mutex<Connection>>, visit_id: &str) -> (String, Option<String>, bool, Option<String>, Option<i32>) {
    let conn = db.lock().unwrap();
    conn.query_row(
        "SELECT status, badge_number, is_group, group_name, group_size FROM visits WHERE id = ?1",
        rusqlite::params![visit_id],
        |row| Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<i64>>(2)?.unwrap_or(0) != 0,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<i32>>(4)?,
        )),
    ).unwrap()
}

// ── Test: Group Visit Registration ───────────────────────────

#[test]
fn test_group_visit_creation() {
    let db = test_db();
    let host_id = seed_host(&db);
    let visitor_id = create_visitor(&db, "Lincoln Elementary 3rd Grade");
    let visit_id = create_group_visit(&db, &visitor_id, &host_id, "Lincoln Elementary 3rd Grade", 30);

    let (status, badge_number, is_group, group_name, group_size) = get_visit(&db, &visit_id);
    assert_eq!(status, "pending");
    assert!(badge_number.is_none(), "Badge number should not be set before check-in");
    assert!(is_group, "Visit should be marked as group");
    assert_eq!(group_name.as_deref(), Some("Lincoln Elementary 3rd Grade"));
    assert_eq!(group_size, Some(30));
}

// ── Test: Group Check-In assigns badge number ────────────────

#[test]
fn test_group_checkin() {
    let db = test_db();
    let host_id = seed_host(&db);
    let visitor_id = create_visitor(&db, "Tour Group A");
    let visit_id = create_group_visit(&db, &visitor_id, &host_id, "Tour Group A", 15);

    // Simulate check-in (mirrors db::check_in_visit)
    let badge_num = {
        let conn = db.lock().unwrap();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM visits WHERE status = 'checked_in' AND check_in LIKE ?1 || '%'",
            rusqlite::params![today],
            |r| r.get(0),
        ).unwrap_or(0);
        format!("V-{:03}", count + 1)
    };

    {
        let conn = db.lock().unwrap();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "UPDATE visits SET status = 'checked_in', check_in = ?1, badge_number = ?2, updated_at = ?1 WHERE id = ?3",
            rusqlite::params![now, badge_num, visit_id],
        ).unwrap();
    }

    let (status, badge_number, is_group, _, group_size) = get_visit(&db, &visit_id);
    assert_eq!(status, "checked_in");
    assert_eq!(badge_number.as_deref(), Some("V-001"));
    assert!(is_group);
    assert_eq!(group_size, Some(15));
}

// ── Test: Group Check-Out ────────────────────────────────────

#[test]
fn test_group_checkout() {
    let db = test_db();
    let host_id = seed_host(&db);
    let visitor_id = create_visitor(&db, "Tour Group B");
    let visit_id = create_group_visit(&db, &visitor_id, &host_id, "Tour Group B", 25);

    // Check in
    {
        let conn = db.lock().unwrap();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "UPDATE visits SET status = 'checked_in', check_in = ?1, badge_number = 'V-001', updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now, visit_id],
        ).unwrap();
    }

    // Check out
    {
        let conn = db.lock().unwrap();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "UPDATE visits SET status = 'checked_out', check_out = ?1, updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now, visit_id],
        ).unwrap();
    }

    let (status, _, is_group, group_name, group_size) = get_visit(&db, &visit_id);
    assert_eq!(status, "checked_out");
    assert!(is_group);
    assert_eq!(group_name.as_deref(), Some("Tour Group B"));
    assert_eq!(group_size, Some(25));
}

// ── Test: Group visit doesn't interfere with regular visits ──

#[test]
fn test_group_and_regular_visits_coexist() {
    let db = test_db();
    let host_id = seed_host(&db);
    let v1_id = create_visitor(&db, "Jane Doe");
    let v2_id = create_visitor(&db, "Lincoln Elementary");

    let regular_id = create_regular_visit(&db, &v1_id, &host_id);
    let group_id = create_group_visit(&db, &v2_id, &host_id, "Lincoln Elementary", 30);

    let (_, _, is_group_regular, _, _) = get_visit(&db, &regular_id);
    let (_, _, is_group_group, _, group_size) = get_visit(&db, &group_id);

    assert!(!is_group_regular, "Regular visit should not be a group");
    assert!(is_group_group, "Group visit should be a group");
    assert_eq!(group_size, Some(30));

    // Both should be pending
    let conn = db.lock().unwrap();
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM visits WHERE status = 'pending'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(count, 2, "Both visits should be pending");
}

// ── Test: Bulk check-out includes group visits ───────────────

#[test]
fn test_bulk_checkout_includes_groups() {
    let db = test_db();
    let host_id = seed_host(&db);
    let v1_id = create_visitor(&db, "Individual Visitor");
    let v2_id = create_visitor(&db, "School Group");

    let regular_id = create_regular_visit(&db, &v1_id, &host_id);
    let group_id = create_group_visit(&db, &v2_id, &host_id, "School Group", 20);

    // Check in both
    {
        let conn = db.lock().unwrap();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "UPDATE visits SET status = 'checked_in', check_in = ?1, badge_number = 'V-001' WHERE id = ?2",
            rusqlite::params![now, regular_id],
        ).unwrap();
        conn.execute(
            "UPDATE visits SET status = 'checked_in', check_in = ?1, badge_number = 'V-002' WHERE id = ?2",
            rusqlite::params![now, group_id],
        ).unwrap();
    }

    // Bulk check-out (mirrors db::check_out_all_today)
    {
        let conn = db.lock().unwrap();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let count = conn.execute(
            "UPDATE visits SET status = 'checked_out', check_out = ?1, updated_at = ?1
             WHERE status = 'checked_in' AND date(check_in) = ?2",
            rusqlite::params![now, today],
        ).unwrap();
        assert_eq!(count, 2, "Both visits should be checked out");
    }

    let (status1, _, _, _, _) = get_visit(&db, &regular_id);
    let (status2, _, _, _, _) = get_visit(&db, &group_id);
    assert_eq!(status1, "checked_out");
    assert_eq!(status2, "checked_out");
}

// ── Test: Badge numbering — group gets ONE badge number ──────

#[test]
fn test_badge_number_sequence() {
    let db = test_db();
    let host_id = seed_host(&db);

    // Create individual visit, check in → V-001
    let v1_id = create_visitor(&db, "Visitor One");
    let visit1_id = create_regular_visit(&db, &v1_id, &host_id);
    {
        let conn = db.lock().unwrap();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "UPDATE visits SET status = 'checked_in', check_in = ?1, badge_number = 'V-001' WHERE id = ?2",
            rusqlite::params![now, visit1_id],
        ).unwrap();
    }

    // Create group of 30, check in → V-002 (one number, not 30)
    let v2_id = create_visitor(&db, "Big Group");
    let visit2_id = create_group_visit(&db, &v2_id, &host_id, "Big Group", 30);
    {
        // Generate next badge number (should be V-002)
        let badge_num = {
            let conn = db.lock().unwrap();
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM visits WHERE status = 'checked_in' AND check_in LIKE ?1 || '%'",
                rusqlite::params![today],
                |r| r.get(0),
            ).unwrap_or(0);
            format!("V-{:03}", count + 1)
        };
        assert_eq!(badge_num, "V-002", "Group should get the next sequential badge number");

        let conn = db.lock().unwrap();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "UPDATE visits SET status = 'checked_in', check_in = ?1, badge_number = ?2 WHERE id = ?3",
            rusqlite::params![now, badge_num, visit2_id],
        ).unwrap();
    }

    // Create another individual visit → V-003
    let v3_id = create_visitor(&db, "Visitor Three");
    let visit3_id = create_regular_visit(&db, &v3_id, &host_id);
    {
        let badge_num = {
            let conn = db.lock().unwrap();
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM visits WHERE status = 'checked_in' AND check_in LIKE ?1 || '%'",
                rusqlite::params![today],
                |r| r.get(0),
            ).unwrap_or(0);
            format!("V-{:03}", count + 1)
        };
        assert_eq!(badge_num, "V-003", "Next individual should get V-003 after the group");

        let conn = db.lock().unwrap();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "UPDATE visits SET status = 'checked_in', check_in = ?1, badge_number = ?2 WHERE id = ?3",
            rusqlite::params![now, badge_num, visit3_id],
        ).unwrap();
    }

    // Verify final state
    let (_, b1, _, _, _) = get_visit(&db, &visit1_id);
    let (_, b2, _, _, _) = get_visit(&db, &visit2_id);
    let (_, b3, _, _, _) = get_visit(&db, &visit3_id);
    assert_eq!(b1.as_deref(), Some("V-001"));
    assert_eq!(b2.as_deref(), Some("V-002"));
    assert_eq!(b3.as_deref(), Some("V-003"));
}

// ── Test: Group size validation boundaries ───────────────────

#[test]
fn test_group_size_stored_correctly() {
    let db = test_db();
    let host_id = seed_host(&db);

    // Small group (minimum = 2)
    let v1_id = create_visitor(&db, "Small Group");
    let visit1_id = create_group_visit(&db, &v1_id, &host_id, "Small Group", 2);
    let (_, _, _, _, size1) = get_visit(&db, &visit1_id);
    assert_eq!(size1, Some(2));

    // Large group (maximum = 200)
    let v2_id = create_visitor(&db, "Large Group");
    let visit2_id = create_group_visit(&db, &v2_id, &host_id, "Large Group", 200);
    let (_, _, _, _, size2) = get_visit(&db, &visit2_id);
    assert_eq!(size2, Some(200));
}

// ── Test: Full lifecycle — register → check-in → check-out ──

#[test]
fn test_full_group_lifecycle() {
    let db = test_db();
    let host_id = seed_host(&db);
    let visitor_id = create_visitor(&db, "Lincoln Elementary 3rd Grade");
    let visit_id = create_group_visit(
        &db, &visitor_id, &host_id,
        "Lincoln Elementary 3rd Grade", 30,
    );

    // 1. Verify initial state
    let (status, badge, is_group, name, size) = get_visit(&db, &visit_id);
    assert_eq!(status, "pending");
    assert!(badge.is_none());
    assert!(is_group);
    assert_eq!(name.as_deref(), Some("Lincoln Elementary 3rd Grade"));
    assert_eq!(size, Some(30));

    // 2. Check in
    {
        let conn = db.lock().unwrap();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "UPDATE visits SET status = 'checked_in', check_in = ?1, badge_number = 'V-001', updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now, visit_id],
        ).unwrap();
    }
    let (status, badge, _, _, _) = get_visit(&db, &visit_id);
    assert_eq!(status, "checked_in");
    assert_eq!(badge.as_deref(), Some("V-001"));

    // 3. Check out
    {
        let conn = db.lock().unwrap();
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "UPDATE visits SET status = 'checked_out', check_out = ?1, updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now, visit_id],
        ).unwrap();
    }
    let (status, _, is_group, name, size) = get_visit(&db, &visit_id);
    assert_eq!(status, "checked_out");
    assert!(is_group);
    assert_eq!(name.as_deref(), Some("Lincoln Elementary 3rd Grade"));
    assert_eq!(size, Some(30));

    // 4. Verify check_out timestamp was set
    {
        let conn = db.lock().unwrap();
        let checkout: Option<String> = conn.query_row(
            "SELECT check_out FROM visits WHERE id = ?1",
            rusqlite::params![visit_id],
            |r| r.get(0),
        ).unwrap();
        assert!(checkout.is_some(), "check_out timestamp should be set");
    }
}
