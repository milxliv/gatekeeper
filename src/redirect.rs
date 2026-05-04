use std::net::SocketAddr;

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode, Uri},
    response::Redirect,
    routing::any,
    Router,
};

async fn redirect(
    State(https_port): State<u16>,
    headers: HeaderMap,
    uri: Uri,
) -> Result<Redirect, StatusCode> {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let host_no_port = host.split(':').next().unwrap_or(host);
    let path = uri.path_and_query().map(|p| p.as_str()).unwrap_or("");
    let target = if https_port == 443 {
        format!("https://{host_no_port}{path}")
    } else {
        format!("https://{host_no_port}:{https_port}{path}")
    };
    Ok(Redirect::permanent(&target))
}

/// Spawn a tiny HTTP server that 308-redirects every request to the same
/// host on `https_port`. Runs in the background; if binding fails (port 80
/// already in use, or insufficient privilege on Windows) it logs a warning
/// and exits the task so the main TLS server can still come up.
pub fn spawn(http_port: u16, https_port: u16) {
    tokio::spawn(async move {
        let app = Router::new()
            .fallback(any(redirect))
            .with_state(https_port);

        let addr = SocketAddr::from(([0, 0, 0, 0], http_port));
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!(
                    "HTTP→HTTPS redirect disabled: could not bind :{http_port} ({e}). \
                     On Windows, :80 may need admin rights or be held by IIS/W3SVC. \
                     Set GATEKEEPER_HTTP_REDIRECT_PORT=8080 to use an alt port, or =0 to disable."
                );
                return;
            }
        };

        tracing::info!(
            "HTTP→HTTPS redirect listening on :{http_port} → https://*:{https_port}"
        );
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("HTTP redirect server failed: {e}");
        }
    });
}
