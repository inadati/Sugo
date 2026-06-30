//! Local callback HTTP server for Nipper liveness callbacks.
//!
//! Each sugo-mcp process binds an ephemeral port and serves:
//!   POST /heartbeat      {run_id}          -> records last_heartbeat_at
//!   POST /session-event  {run_id, reason}  -> updates run status
//! The bound URL is handed to Nipper as `sugo_callback_url` at /attach time.

use std::sync::Arc;

use axum::{extract::State, routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sugo_core::domain::run::RunStatus;
use sugo_core::ports::id_clock::IdClock;
use sugo_core::ports::run_repository::RunRepository;
use sugo_infra::sqlite::SqliteRunRepository;

use crate::tools::RealIdClock;

#[derive(Clone)]
pub struct CallbackState {
    pub run_repo: Arc<SqliteRunRepository>,
    pub clock: Arc<RealIdClock>,
    pub nipper_base: String,
}

#[derive(Deserialize)]
struct HeartbeatReq {
    run_id: String,
}

#[derive(Deserialize)]
struct SessionEventReq {
    run_id: String,
    reason: String,
}

#[derive(Deserialize)]
struct InjectAckReq {
    run_id: String,
}

/// Map a Nipper session-event reason to the new run status, or None to ignore.
pub fn status_for_reason(reason: &str) -> Option<RunStatus> {
    match reason {
        "user_closed" => Some(RunStatus::Closed),
        "session_exited" => Some(RunStatus::Disconnected),
        _ => None,
    }
}

async fn handle_heartbeat(State(st): State<CallbackState>, Json(req): Json<HeartbeatReq>) -> Json<Value> {
    let now = st.clock.now_iso();
    let _ = st.run_repo.update_heartbeat(&req.run_id, &now).await;
    Json(json!({ "status": "ok" }))
}

async fn handle_inject_ack(State(st): State<CallbackState>, Json(req): Json<InjectAckReq>) -> Json<Value> {
    let _ = st.run_repo.set_inject_pending(&req.run_id, None).await;
    // After ack, monitor jsonl and remind agent to call sugo_advance if it forgets.
    let since_iso = st.clock.now_iso();
    if let Ok(Some(run)) = st.run_repo.get(&req.run_id).await
        && let Some(project_path) = run.project_path
    {
        crate::advance_reminder::spawn(
            req.run_id,
            project_path,
            st.run_repo.clone(),
            st.nipper_base.clone(),
            since_iso,
        );
    }
    Json(json!({ "status": "ok" }))
}

async fn handle_session_event(State(st): State<CallbackState>, Json(req): Json<SessionEventReq>) -> Json<Value> {
    if let Some(status) = status_for_reason(&req.reason)
        && let Ok(Some(mut run)) = st.run_repo.get(&req.run_id).await
    {
        run.status = status;
        run.updated_at = st.clock.now_iso();
        let _ = st.run_repo.update(&run).await;
    }
    Json(json!({ "status": "ok" }))
}

/// Build the callback router with shared state.
pub fn router(state: CallbackState) -> Router {
    Router::new()
        .route("/heartbeat", post(handle_heartbeat))
        .route("/inject-ack", post(handle_inject_ack))
        .route("/session-event", post(handle_session_event))
        .with_state(state)
}

/// Bind an ephemeral local port and start the callback server.
/// Returns the callback base URL (e.g. "http://127.0.0.1:54321").
pub async fn start(state: CallbackState) -> anyhow::Result<String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let app = router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(format!("http://{addr}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sugo_infra::sqlite::schema::SCHEMA;
    use std::sync::Mutex;

    fn state() -> CallbackState {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        CallbackState {
            run_repo: Arc::new(SqliteRunRepository::new(Mutex::new(conn))),
            clock: Arc::new(RealIdClock),
            nipper_base: "http://127.0.0.1:8771".into(),
        }
    }

    #[test]
    fn reason_mapping() {
        assert_eq!(status_for_reason("user_closed"), Some(RunStatus::Closed));
        assert_eq!(status_for_reason("session_exited"), Some(RunStatus::Disconnected));
        assert_eq!(status_for_reason("weird"), None);
    }

    #[tokio::test]
    async fn heartbeat_updates_last_heartbeat_at() {
        let st = state();
        st.run_repo.create(&{
            use sugo_core::domain::run::Run;
            Run { id: "r1".into(), harness_id: "h1".into(), board_version_no: 1,
                current_cell_id: "c1".into(), status: RunStatus::Running,
                project_path: Some("/p".into()), created_at: "t".into(), updated_at: "t".into(),
                last_heartbeat_at: None, inject_pending_since: None }
        }).await.unwrap();
        let _ = handle_heartbeat(State(st.clone()), Json(HeartbeatReq { run_id: "r1".into() })).await;
        let got = st.run_repo.get("r1").await.unwrap().unwrap();
        assert!(got.last_heartbeat_at.is_some());
    }

    #[tokio::test]
    async fn session_event_user_closed_sets_closed() {
        let st = state();
        st.run_repo.create(&{
            use sugo_core::domain::run::Run;
            Run { id: "r1".into(), harness_id: "h1".into(), board_version_no: 1,
                current_cell_id: "c1".into(), status: RunStatus::Running,
                project_path: Some("/p".into()), created_at: "t".into(), updated_at: "t".into(),
                last_heartbeat_at: None, inject_pending_since: None }
        }).await.unwrap();
        let _ = handle_session_event(State(st.clone()), Json(SessionEventReq { run_id: "r1".into(), reason: "user_closed".into() })).await;
        let got = st.run_repo.get("r1").await.unwrap().unwrap();
        assert_eq!(got.status, RunStatus::Closed);
    }
}
