//! Outbound client for the Nipper local inject API (127.0.0.1:8771).
//!
//! All calls are localhost HTTP. Responses are classified into a small result
//! enum the MCP handlers map to tool errors.

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
/// Returns Err if the file cannot be read (e.g. Nipper not running).
fn read_token(path: &str) -> Result<String, ()> {
    std::fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .map_err(|_| ())
}

async fn post(base: &str, path: &str, body: serde_json::Value) -> NipperOutcome {
    let client = reqwest::Client::new();
    match client.post(format!("{base}{path}")).json(&body).send().await {
        Ok(resp) => classify(resp.status().as_u16()),
        Err(_) => NipperOutcome::Unreachable,
    }
}

pub async fn attach(base: &str, project_path: &str, run_id: &str, callback_url: &str) -> NipperOutcome {
    post(base, "/attach", json!({
        "project_path": project_path, "run_id": run_id, "sugo_callback_url": callback_url
    })).await
}

pub async fn inject(base: &str, project_path: &str, text: &str) -> NipperOutcome {
    post(base, "/inject", json!({ "project_path": project_path, "text": text })).await
}

pub async fn detach(base: &str, project_path: &str) -> NipperOutcome {
    post(base, "/detach", json!({ "project_path": project_path })).await
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
        assert_eq!(inject(&base, "/p", "hi").await, NipperOutcome::Ok);
    }

    #[tokio::test]
    async fn inject_404_is_no_session() {
        let base = spawn_mock(StatusCode::NOT_FOUND).await;
        assert_eq!(inject(&base, "/p", "hi").await, NipperOutcome::NoSession);
    }

    #[tokio::test]
    async fn unreachable_when_no_server() {
        // Port 1 is reserved/unbound; connection fails.
        assert_eq!(
            inject("http://127.0.0.1:1", "/p", "hi").await,
            NipperOutcome::Unreachable
        );
    }

    #[test]
    fn classify_maps_statuses() {
        assert_eq!(classify(200), NipperOutcome::Ok);
        assert_eq!(classify(404), NipperOutcome::NoSession);
        assert_eq!(classify(401), NipperOutcome::Unauthorized);
        assert_eq!(classify(400), NipperOutcome::BadRequest);
    }

    #[test]
    fn read_token_reads_trimmed_contents() {
        let path = write_temp_token("abc123\n");
        assert_eq!(read_token(&path), Ok("abc123".to_string()));
    }

    #[test]
    fn read_token_missing_file_is_err() {
        let path = std::env::temp_dir().join(format!("sugo-test-missing-{}", uuid::Uuid::new_v4()));
        assert_eq!(read_token(path.to_str().unwrap()), Err(()));
    }
}
