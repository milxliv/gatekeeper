mod db;
mod email;
mod graph;
mod graph_service;
mod models;
mod routes;
mod templates;

use std::sync::Arc;
use axum::{
    http::header,
    response::IntoResponse,
    routing::{delete, get, post},
    Router,
};
use std::net::SocketAddr;

const HTMX_JS: &[u8] = include_bytes!("htmx.min.js");

async fn serve_htmx() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/javascript")], HTMX_JS)
}

pub struct AppState {
    pub db: db::DbPool,
    pub graph: Option<Arc<graph_service::GraphService>>,
    pub photos_dir: std::path::PathBuf,
}

#[tokio::main]
async fn main() {
    // Load .env file if present (dev convenience)
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Initialize SQLite
    let db_path = std::env::var("GATEKEEPER_DB")
        .unwrap_or_else(|_| "gatekeeper.db".to_string());

    let pool = db::init_db(&db_path).expect("Failed to initialize database");

    tracing::info!("Database initialized at {}", db_path);

    // Seed some demo hosts if the table is empty
    seed_demo_data(&pool);

    // Graph service — optional, gracefully disabled if env vars missing
    let graph = match graph_service::GraphService::from_env() {
        Ok(svc) => {
            tracing::info!("Graph calendar integration enabled");
            Some(Arc::new(svc))
        }
        Err(e) => {
            tracing::warn!(
                "Graph calendar disabled (set GRAPH_* env vars to enable): {e}"
            );
            None
        }
    };

    let photos_dir = std::path::PathBuf::from(
        std::env::var("GATEKEEPER_PHOTOS").unwrap_or_else(|_| "photos".to_string())
    );
    std::fs::create_dir_all(&photos_dir).expect("Failed to create photos directory");

    let state = Arc::new(AppState { db: pool, graph, photos_dir });

    // Background photo cleanup task
    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            // Run cleanup on startup, then every hour
            loop {
                let hours: i64 = db::get_setting(&state.db, "photo_retention_hours")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(24);

                if hours > 0 {
                    let files = db::expired_photo_filenames(&state.db, hours);
                    if !files.is_empty() {
                        let mut deleted = 0usize;
                        for filename in &files {
                            let path = state.photos_dir.join(filename);
                            if std::fs::remove_file(&path).is_ok() {
                                deleted += 1;
                            }
                        }
                        let cleared = db::clear_expired_photos(&state.db, hours);
                        tracing::info!(
                            "Photo cleanup: deleted {} files, cleared {} DB records (retention={}h)",
                            deleted, cleared, hours
                        );
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });
    }

    let app = Router::new()
        // Embedded static assets
        .route("/static/htmx.min.js", get(serve_htmx))
        // Page routes
        .route("/", get(routes::page_dashboard))
        .route("/pre-register", get(routes::page_pre_register))
        .route("/walk-in", get(routes::page_walk_in))
        .route("/hosts", get(routes::page_hosts))
        .route("/log", get(routes::page_log))
        .route("/admin", get(routes::page_admin))
        .route("/admin/settings", post(routes::api_save_general_settings))
        .route("/admin/settings/graph", post(routes::api_save_graph_settings))
        .route("/admin/settings/smtp", post(routes::api_save_smtp_settings))
        .route("/admin/settings/smtp/test", post(routes::api_test_smtp))
        .route("/admin/settings/theme", post(routes::api_save_theme))
        .route("/admin/settings/dropdowns", post(routes::api_save_dropdowns))
        .route("/admin/settings/badge", post(routes::api_save_badge_branding))
        .route("/admin/settings/badge/logo", post(routes::api_upload_logo))
        .route("/badge/preview", get(routes::page_badge_preview))
        // API routes (HTMX partials)
        .route("/api/dashboard/today", get(routes::api_dashboard_today))
        .route("/api/pre-register", post(routes::api_pre_register))
        .route("/api/walk-in", post(routes::api_walk_in))
        .route("/api/hosts", post(routes::api_add_host))
        .route("/api/hosts/:id", post(routes::api_update_host).delete(routes::api_delete_host))
        .route("/api/visits/:id/approve", post(routes::api_approve_visit))
        .route("/api/visits/:id/deny", post(routes::api_deny_visit))
        .route("/api/visits/:id/late", post(routes::api_late_visit))
        .route("/api/visits/:id/reschedule", post(routes::api_reschedule_visit))
        .route("/api/visits/:id/checkin", post(routes::api_checkin_visit))
        .route("/api/visits/:id/checkout", post(routes::api_checkout_visit))
        .route("/api/log/search", get(routes::api_log_search))
        // Kiosk JSON API
        .route("/api/kiosk/checkin", post(routes::api_kiosk_checkin))
        // Badge printing
        .route("/badge/:id", get(routes::page_badge))
        // Photo capture
        .route("/api/visits/:id/visitor-id", get(routes::api_visit_visitor_id))
        .route("/api/visitors/search", get(routes::api_search_visitors))
        .route("/api/hosts/search", get(routes::api_search_hosts))
        .route("/api/visitors/:id/photo", post(routes::api_upload_photo))
        .route("/photos/:filename", get(routes::serve_photo))
        .with_state(state);

    let port: u16 = std::env::var("GATEKEEPER_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("GateKeeper running at http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn seed_demo_data(pool: &db::DbPool) {
    let hosts = db::list_hosts(pool).unwrap_or_default();
    if !hosts.is_empty() {
        return;
    }

    tracing::info!("Seeding demo host data...");

    let demo_hosts = vec![
        models::NewHost {
            name: "Adam (Engineering)".to_string(),
            department: "Engineering".to_string(),
            email: "engineering@wbbh.com".to_string(),
            phone: None,
        },
        models::NewHost {
            name: "Front Desk".to_string(),
            department: "Management".to_string(),
            email: "frontdesk@wbbh.com".to_string(),
            phone: None,
        },
        models::NewHost {
            name: "News Director".to_string(),
            department: "News".to_string(),
            email: "news@wbbh.com".to_string(),
            phone: None,
        },
        models::NewHost {
            name: "Sales Manager".to_string(),
            department: "Sales".to_string(),
            email: "sales@wbbh.com".to_string(),
            phone: None,
        },
    ];

    for host in &demo_hosts {
        let _ = db::insert_host(pool, host);
    }
}
