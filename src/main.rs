mod db;
mod email;
mod graph;
mod graph_service;
mod models;
mod rate_limit;
mod redirect;
mod routes;
mod templates;
mod tls;

use std::sync::Arc;
use axum::{
    extract::{DefaultBodyLimit, State},
    http::header,
    middleware,
    response::IntoResponse,
    routing::{get, post},
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
    pub password_hash: Option<String>,
    pub admin_password_hash: Option<String>,
    pub kiosk_secret: Option<String>,
    pub auth_attempts: Arc<rate_limit::AuthAttemptTracker>,
}

/// User role, injected into request extensions by auth middleware.
#[derive(Clone, Debug)]
pub struct UserRole(pub String); // "admin" or "user"

/// Reception auth middleware — no admin routes exist on this port.
/// If no password is set, passes through as "user" (no admin on reception).
async fn require_reception_auth(
    State(state): State<Arc<AppState>>,
    mut request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    // If no password configured, pass through as user
    if state.password_hash.is_none() {
        request.extensions_mut().insert(UserRole("user".to_string()));
        return next.run(request).await;
    }

    let path = request.uri().path().to_string();
    if path == "/login"
        || path.starts_with("/static/")
        || path == "/api/kiosk/checkin"
        || path.starts_with("/badge/")
        || path == "/photos/logo.png"
    {
        request.extensions_mut().insert(UserRole("user".to_string()));
        return next.run(request).await;
    }

    if let Some(cookie_header) = request.headers().get("cookie") {
        if let Ok(cookies) = cookie_header.to_str() {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix("gk_session=") {
                    if db::validate_session(&state.db, token).is_some() {
                        request
                            .extensions_mut()
                            .insert(UserRole("user".to_string()));
                        return next.run(request).await;
                    }
                }
            }
        }
    }

    axum::response::Redirect::to("/login").into_response()
}

/// Admin auth middleware — validates gk_admin_session cookie.
async fn require_admin_auth(
    State(state): State<Arc<AppState>>,
    mut request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let path = request.uri().path().to_string();

    // Public paths on the admin port
    if path == "/login"
        || path.starts_with("/static/")
        || path == "/totp/confirm"
    {
        request
            .extensions_mut()
            .insert(UserRole("admin".to_string()));
        return next.run(request).await;
    }

    // Validate admin session cookie
    if let Some(cookie_header) = request.headers().get("cookie") {
        if let Ok(cookies) = cookie_header.to_str() {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(token) = cookie.strip_prefix("gk_admin_session=") {
                    if let Some(role) = db::validate_session(&state.db, token) {
                        if role == "admin" {
                            request
                                .extensions_mut()
                                .insert(UserRole("admin".to_string()));
                            return next.run(request).await;
                        }
                    }
                }
            }
        }
    }

    axum::response::Redirect::to("/login").into_response()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let db_path = std::env::var("GATEKEEPER_DB")
        .unwrap_or_else(|_| "gatekeeper.db".to_string());

    let pool = db::init_db(&db_path).expect("Failed to initialize database");

    tracing::info!("Database initialized at {}", db_path);

    seed_demo_data(&pool);

    let hash_password = |pw: &str| -> String {
        use argon2::password_hash::{PasswordHasher, SaltString, rand_core};
        let salt = SaltString::generate(&mut rand_core::OsRng);
        argon2::Argon2::default()
            .hash_password(pw.as_bytes(), &salt)
            .expect("Failed to hash password")
            .to_string()
    };

    let password_hash = match std::env::var("GATEKEEPER_PASSWORD") {
        Ok(pw) if !pw.is_empty() => {
            tracing::info!("Front desk authentication enabled");
            Some(hash_password(&pw))
        }
        _ => {
            tracing::warn!(
                "GATEKEEPER_PASSWORD not set — reception dashboard is unprotected! \
                 Set this env var to enable login."
            );
            None
        }
    };

    let admin_password_hash = match std::env::var("GATEKEEPER_ADMIN_PASSWORD") {
        Ok(pw) if !pw.is_empty() => {
            tracing::info!("Admin authentication enabled (separate admin password)");
            Some(hash_password(&pw))
        }
        _ => {
            tracing::warn!(
                "GATEKEEPER_ADMIN_PASSWORD not set — admin port will reject \
                 all logins. Set this env var to enable admin access."
            );
            None
        }
    };

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
        std::env::var("GATEKEEPER_PHOTOS")
            .unwrap_or_else(|_| "photos".to_string()),
    );
    std::fs::create_dir_all(&photos_dir).expect("Failed to create photos directory");

    let kiosk_secret = match std::env::var("GATEKEEPER_KIOSK_SECRET") {
        Ok(s) if !s.is_empty() => {
            tracing::info!("Kiosk API secret configured");
            Some(s)
        }
        _ => {
            tracing::warn!(
                "GATEKEEPER_KIOSK_SECRET not set — kiosk endpoint is unprotected!"
            );
            None
        }
    };

    let state = Arc::new(AppState {
        db: pool,
        graph,
        photos_dir,
        password_hash,
        admin_password_hash,
        kiosk_secret,
        auth_attempts: Arc::new(rate_limit::AuthAttemptTracker::new()),
    });

    let promoted = db::promote_rescheduled_visits(&state.db);
    if promoted > 0 {
        tracing::info!("Promoted {} rescheduled visit(s) to pending", promoted);
    }

    // Background cleanup task
    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
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
                            "Photo cleanup: deleted {} files, cleared {} DB records \
                             (retention={}h)",
                            deleted, cleared, hours
                        );
                    }
                }

                db::cleanup_expired_sessions(&state.db);

                state
                    .auth_attempts
                    .sweep(std::time::Duration::from_secs(900));

                // Path A retention: purge checked-out visits + orphaned
                // visitors after the configured window (default 8h).
                let visit_hours: i64 =
                    db::get_setting(&state.db, "visit_retention_hours")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(8);
                if visit_hours > 0 {
                    let res = db::purge_old_visits(&state.db, visit_hours);
                    let mut photos_unlinked = 0usize;
                    for filename in &res.photo_filenames {
                        let path = state.photos_dir.join(filename);
                        if std::fs::remove_file(&path).is_ok() {
                            photos_unlinked += 1;
                        }
                    }
                    if res.visits_deleted > 0
                        || res.visitors_deleted > 0
                        || photos_unlinked > 0
                    {
                        tracing::info!(
                            "Visit retention sweep: purged {} visit(s), {} \
                             orphan visitor(s), {} photo file(s) (window={}h)",
                            res.visits_deleted,
                            res.visitors_deleted,
                            photos_unlinked,
                            visit_hours
                        );
                    }
                }

                let promoted = db::promote_rescheduled_visits(&state.db);
                if promoted > 0 {
                    tracing::info!(
                        "Promoted {} rescheduled visit(s) to pending",
                        promoted
                    );
                }

                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            }
        });
    }

    // ── Reception router (port 3000) ─────────────────────────
    let reception_app = Router::new()
        .route("/login", get(routes::page_login).post(routes::api_login))
        .route("/logout", post(routes::api_logout))
        .route("/static/htmx.min.js", get(serve_htmx))
        .route("/", get(routes::page_dashboard))
        .route("/pre-register", get(routes::page_pre_register))
        .route("/walk-in", get(routes::page_walk_in))
        .route("/group-visit", get(routes::page_group_visit))
        .route("/hosts", get(routes::page_hosts))
        .route("/log", get(routes::page_log))
        // API routes (HTMX partials)
        .route("/api/dashboard/today", get(routes::api_dashboard_today))
        .route("/api/pre-register", post(routes::api_pre_register))
        .route("/api/walk-in", post(routes::api_walk_in))
        .route("/api/group-visit", post(routes::api_group_visit))
        .route(
            "/api/visits/:id/approve",
            post(routes::api_approve_visit),
        )
        .route("/api/visits/:id/deny", post(routes::api_deny_visit))
        .route("/api/visits/:id/late", post(routes::api_late_visit))
        .route(
            "/api/visits/:id/reschedule",
            post(routes::api_reschedule_visit),
        )
        .route(
            "/api/visits/:id/checkin",
            post(routes::api_checkin_visit),
        )
        .route(
            "/api/visits/checkout-all",
            post(routes::api_checkout_all),
        )
        .route(
            "/api/visits/:id/checkout",
            post(routes::api_checkout_visit),
        )
        .route("/api/log/search", get(routes::api_log_search))
        // Kiosk JSON API
        .route("/api/kiosk/checkin", post(routes::api_kiosk_checkin))
        // Badge printing
        .route("/badge/:id", get(routes::page_badge))
        .route("/badge/preview", get(routes::page_badge_preview))
        // Photo capture
        .route(
            "/api/visits/:id/visitor-id",
            get(routes::api_visit_visitor_id),
        )
        .route("/api/visitors/search", get(routes::api_search_visitors))
        .route("/api/hosts/search", get(routes::api_search_hosts))
        .route(
            "/api/visitors/:id/photo",
            post(routes::api_upload_photo),
        )
        .route("/photos/:filename", get(routes::serve_photo))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_reception_auth,
        ))
        .with_state(state.clone());

    // ── Admin router (port 3001) ─────────────────────────────
    let admin_app = Router::new()
        .route(
            "/login",
            get(routes::page_admin_login).post(routes::api_admin_login),
        )
        .route("/logout", post(routes::api_admin_logout))
        .route("/totp/confirm", post(routes::api_admin_totp_confirm))
        .route("/static/htmx.min.js", get(serve_htmx))
        // Admin pages
        .route("/admin", get(routes::page_admin))
        .route(
            "/admin/settings",
            post(routes::api_save_general_settings),
        )
        .route(
            "/admin/settings/smtp",
            post(routes::api_save_smtp_settings),
        )
        .route(
            "/admin/settings/smtp/test",
            post(routes::api_test_smtp),
        )
        .route(
            "/admin/settings/theme",
            post(routes::api_save_theme),
        )
        .route(
            "/admin/settings/dropdowns",
            post(routes::api_save_dropdowns),
        )
        .route(
            "/admin/settings/badge",
            post(routes::api_save_badge_branding),
        )
        .route(
            "/admin/settings/badge/logo",
            post(routes::api_upload_logo),
        )
        .route("/badge/preview", get(routes::page_badge_preview))
        // Host management (admin-only)
        .route("/hosts", get(routes::page_hosts))
        .route("/api/hosts", post(routes::api_add_host))
        .route(
            "/api/hosts/:id",
            post(routes::api_update_host).delete(routes::api_delete_host),
        )
        .route("/api/hosts/search", get(routes::api_search_hosts))
        // Photos (needed for badge preview with logos)
        .route("/photos/:filename", get(routes::serve_photo))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_admin_auth,
        ))
        .with_state(state.clone());

    // ── TLS setup ──────────────────────────────────────────
    let cert_path = std::path::PathBuf::from(
        std::env::var("GATEKEEPER_TLS_CERT").unwrap_or_else(|_| "tls/cert.pem".to_string()),
    );
    let key_path = std::path::PathBuf::from(
        std::env::var("GATEKEEPER_TLS_KEY").unwrap_or_else(|_| "tls/key.pem".to_string()),
    );

    match tls::ensure_self_signed(&cert_path, &key_path) {
        Ok(true) => tracing::warn!(
            "Generated new self-signed TLS cert at {} (valid for localhost + hostname). \
             Browsers will show a warning until trusted; for production, replace with a \
             CA-signed cert at the same path.",
            cert_path.display()
        ),
        Ok(false) => tracing::info!("Loaded existing TLS cert from {}", cert_path.display()),
        Err(e) => return Err(anyhow::anyhow!("TLS cert setup failed: {e:?}")),
    }

    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("failed to install rustls crypto provider"))?;

    let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert_path, &key_path)
        .await
        .map_err(|e| anyhow::anyhow!("failed to load TLS cert/key: {e}"))?;

    let port: u16 = std::env::var("GATEKEEPER_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3443);

    let admin_port: u16 = std::env::var("GATEKEEPER_ADMIN_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3444);

    let http_redirect_port: u16 = std::env::var("GATEKEEPER_HTTP_REDIRECT_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(80);
    if http_redirect_port > 0 {
        redirect::spawn(http_redirect_port, port);
    }

    let reception_addr = SocketAddr::from(([0, 0, 0, 0], port));
    let admin_addr = SocketAddr::from(([127, 0, 0, 1], admin_port));

    tracing::info!("GateKeeper reception at https://localhost:{}", port);
    tracing::info!(
        "GateKeeper admin at https://127.0.0.1:{} (localhost only)",
        admin_port
    );

    tokio::try_join!(
        axum_server::bind_rustls(reception_addr, tls_config.clone())
            .serve(reception_app.into_make_service_with_connect_info::<SocketAddr>()),
        axum_server::bind_rustls(admin_addr, tls_config)
            .serve(admin_app.into_make_service_with_connect_info::<SocketAddr>()),
    )?;

    Ok(())
}

fn seed_demo_data(pool: &db::DbPool) {
    let hosts = db::list_hosts(pool).unwrap_or_default();
    if !hosts.is_empty() {
        return;
    }

    tracing::info!("Seeding demo host data...");

    // Generic placeholder hosts so the dashboard isn't empty on first run.
    // Replace these in the admin panel before going live.
    let demo_hosts = vec![
        models::NewHost {
            name: "Front Desk".to_string(),
            department: "Reception".to_string(),
            email: "frontdesk@example.com".to_string(),
            phone: None,
        },
        models::NewHost {
            name: "Engineering".to_string(),
            department: "Engineering".to_string(),
            email: "engineering@example.com".to_string(),
            phone: None,
        },
        models::NewHost {
            name: "Operations Manager".to_string(),
            department: "Management".to_string(),
            email: "operations@example.com".to_string(),
            phone: None,
        },
    ];

    for host in &demo_hosts {
        let _ = db::insert_host(pool, host);
    }
}
