//! Outbound client for the Nipper local inject API (127.0.0.1:8771).
//!
//! All calls are localhost HTTP, authenticated with a shared secret token
//! (`X-Nipper-Inject-Token`) that Nipper writes to a local token file at
//! startup; Sugo re-reads that file on every call. Responses are classified
//! into a small result enum the MCP handlers map to tool errors.

use serde_json::json;

pub const NIPPER_BASE_URL: &str = "http://127.0.0.1:8771";

/// Outcome of a Nipper inject-API call.
#[derive(Debug, PartialEq)]
pub enum NipperOutcome {
    Ok,
    NoSession,
    BadRequest,
    Unreachable,
    Unauthorized,
    TokenUnavailable,
    TokenPermissionDenied,
}

/// Map an HTTP status to an outcome (200 => Ok, 404 => NoSession, 401 => Unauthorized, else BadRequest).
fn classify(status: u16) -> NipperOutcome {
    match status {
        200 => NipperOutcome::Ok,
        404 => NipperOutcome::NoSession,
        401 => NipperOutcome::Unauthorized,
        _ => NipperOutcome::BadRequest,
    }
}

/// Read the inject token file, trimming any trailing newline.
/// Returns the io::ErrorKind on failure (e.g. NotFound if Nipper isn't
/// running, PermissionDenied if the file exists but can't be read).
async fn read_token(path: &str) -> Result<String, std::io::ErrorKind> {
    tokio::fs::read_to_string(path)
        .await
        .map(|s| s.trim().to_string())
        .map_err(|e| e.kind())
}

async fn post(base: &str, path: &str, token_path: &str, body: serde_json::Value) -> NipperOutcome {
    let token = match read_token(token_path).await {
        Ok(t) => t,
        Err(std::io::ErrorKind::PermissionDenied) => return NipperOutcome::TokenPermissionDenied,
        Err(_) => return NipperOutcome::TokenUnavailable,
    };
    let client = reqwest::Client::new();
    match client
        .post(format!("{base}{path}"))
        .header("X-Nipper-Inject-Token", token)
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => classify(resp.status().as_u16()),
        Err(_) => NipperOutcome::Unreachable,
    }
}

pub async fn attach(
    base: &str,
    token_path: &str,
    project_path: &str,
    run_id: &str,
    callback_url: &str,
) -> NipperOutcome {
    post(base, "/attach", token_path, json!({
        "project_path": project_path, "run_id": run_id, "sugo_callback_url": callback_url
    })).await
}

pub async fn inject(base: &str, token_path: &str, project_path: &str, text: &str) -> NipperOutcome {
    post(base, "/inject", token_path, json!({ "project_path": project_path, "text": text })).await
}

pub async fn detach(base: &str, token_path: &str, project_path: &str, run_id: &str) -> NipperOutcome {
    post(base, "/detach", token_path, json!({ "project_path": project_path, "run_id": run_id })).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Router, Json, http::StatusCode};
    use serde_json::Value;

    fn write_temp_token(contents: &str) -> String {
        let path = std::env::temp_dir().join(format!("sugo-test-token-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, contents).unwrap();
        path.to_string_lossy().into_owned()
    }

    async fn spawn_mock(status: StatusCode) -> String {
        let app = Router::new().route(
            "/inject",
            post(move |Json(_): Json<Value>| async move { (status, Json(json!({"status":"buffered"}))) }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn inject_200_is_ok() {
        let base = spawn_mock(StatusCode::OK).await;
        let token_path = write_temp_token("tok123");
        assert_eq!(inject(&base, &token_path, "/p", "hi").await, NipperOutcome::Ok);
    }

    #[tokio::test]
    async fn inject_404_is_no_session() {
        let base = spawn_mock(StatusCode::NOT_FOUND).await;
        let token_path = write_temp_token("tok123");
        assert_eq!(inject(&base, &token_path, "/p", "hi").await, NipperOutcome::NoSession);
    }

    #[tokio::test]
    async fn inject_401_is_unauthorized() {
        let base = spawn_mock(StatusCode::UNAUTHORIZED).await;
        let token_path = write_temp_token("tok123");
        assert_eq!(inject(&base, &token_path, "/p", "hi").await, NipperOutcome::Unauthorized);
    }

    #[tokio::test]
    async fn unreachable_when_no_server() {
        // Port 1 is reserved/unbound; connection fails.
        let token_path = write_temp_token("tok123");
        assert_eq!(
            inject("http://127.0.0.1:1", &token_path, "/p", "hi").await,
            NipperOutcome::Unreachable
        );
    }

    #[tokio::test]
    async fn missing_token_file_is_token_unavailable_without_network_call() {
        // Port 1 is reserved/unbound; if a request were attempted it would return
        // Unreachable, not TokenUnavailable — this proves the token read happens
        // before any network call is made.
        let missing_path = std::env::temp_dir().join(format!("sugo-test-missing-{}", uuid::Uuid::new_v4()));
        assert_eq!(
            inject("http://127.0.0.1:1", missing_path.to_str().unwrap(), "/p", "hi").await,
            NipperOutcome::TokenUnavailable
        );
    }

    #[tokio::test]
    async fn inject_sends_token_header() {
        use std::sync::{Arc, Mutex};
        use axum::http::HeaderMap;

        let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let captured_clone = captured.clone();
        let app = Router::new().route(
            "/inject",
            post(move |headers: HeaderMap, Json(_): Json<Value>| {
                let captured = captured_clone.clone();
                async move {
                    let token = headers
                        .get("x-nipper-inject-token")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    *captured.lock().unwrap() = token;
                    (StatusCode::OK, Json(json!({"status":"buffered"})))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
        let base = format!("http://{addr}");
        let token_path = write_temp_token("tok123");

        assert_eq!(inject(&base, &token_path, "/p", "hi").await, NipperOutcome::Ok);
        assert_eq!(captured.lock().unwrap().clone(), Some("tok123".to_string()));
    }

    #[tokio::test]
    async fn detach_sends_run_id_in_body() {
        use std::sync::{Arc, Mutex};

        let captured: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let captured_clone = captured.clone();
        let app = Router::new().route(
            "/detach",
            post(move |Json(body): Json<Value>| {
                let captured = captured_clone.clone();
                async move {
                    *captured.lock().unwrap() = Some(body);
                    (StatusCode::OK, Json(json!({"status":"ok"})))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
        let base = format!("http://{addr}");
        let token_path = write_temp_token("tok123");

        assert_eq!(detach(&base, &token_path, "/p", "run-xyz").await, NipperOutcome::Ok);
        assert_eq!(
            captured.lock().unwrap().as_ref().and_then(|b| b.get("run_id")).and_then(|v| v.as_str()),
            Some("run-xyz")
        );
    }

    #[test]
    fn classify_maps_statuses() {
        assert_eq!(classify(200), NipperOutcome::Ok);
        assert_eq!(classify(404), NipperOutcome::NoSession);
        assert_eq!(classify(401), NipperOutcome::Unauthorized);
        assert_eq!(classify(400), NipperOutcome::BadRequest);
    }

    #[tokio::test]
    async fn read_token_reads_trimmed_contents() {
        let path = write_temp_token("abc123\n");
        assert_eq!(read_token(&path).await, Ok("abc123".to_string()));
    }

    #[tokio::test]
    async fn read_token_missing_file_is_not_found() {
        let path = std::env::temp_dir().join(format!("sugo-test-missing-{}", uuid::Uuid::new_v4()));
        assert_eq!(read_token(path.to_str().unwrap()).await, Err(std::io::ErrorKind::NotFound));
    }

    // NOTE: these two tests assume a non-root test runner. `chmod 0o000` does not
    // block reads for the root user, so if CI ever runs as root (e.g. inside an
    // unmodified container) these would silently pass without exercising the
    // PermissionDenied path (false-negative coverage, not a false failure).
    #[cfg(unix)]
    #[tokio::test]
    async fn read_token_permission_denied_is_distinguished() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("sugo-test-noperm-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, "tok").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let result = read_token(path.to_str().unwrap()).await;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(result, Err(std::io::ErrorKind::PermissionDenied));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn inject_permission_denied_token_is_token_permission_denied_without_network_call() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("sugo-test-noperm-{}", uuid::Uuid::new_v4()));
        std::fs::write(&path, "tok").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let result = inject("http://127.0.0.1:1", path.to_str().unwrap(), "/p", "hi").await;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(result, NipperOutcome::TokenPermissionDenied);
    }

    #[tokio::test]
    async fn inject_400_is_bad_request() {
        let base = spawn_mock(StatusCode::BAD_REQUEST).await;
        let token_path = write_temp_token("tok123");
        assert_eq!(inject(&base, &token_path, "/p", "hi").await, NipperOutcome::BadRequest);
    }
}
