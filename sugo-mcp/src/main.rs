//! Sugo MCP server: exposes the four P1 harness tools over rmcp/stdio.
//!
//! Each tool is a thin adapter that parses its arguments, calls the matching
//! `sugo-core` use case against a `SqliteHarnessRepository`, and serialises the
//! result. Domain errors are mapped to tool errors via [`error::to_tool_error`].

mod advance_reminder;
mod callback;
mod error;
mod nipper_client;
mod tools;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router, transport::stdio};
use std::sync::Arc;
use sugo_core::ports::id_clock::IdClock;
use sugo_core::ports::repository::HarnessRepository;
use sugo_core::ports::run_repository::RunRepository;
use sugo_infra::sqlite::SqliteHarnessRepository;
use sugo_infra::sqlite::SqliteRunRepository;
use tools::RealIdClock;

#[derive(Clone)]
struct SugoServer {
    repo: Arc<SqliteHarnessRepository>,
    run_repo: Arc<SqliteRunRepository>,
    clock: Arc<RealIdClock>,
    /// Callback base URL this process advertises to Nipper at /attach time.
    callback_url: String,
    /// Nipper inject API base URL.
    nipper_base: String,
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for SugoServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SugoServer").finish()
    }
}

#[tool_router]
impl SugoServer {
    fn new(
        repo: Arc<SqliteHarnessRepository>,
        run_repo: Arc<SqliteRunRepository>,
        callback_url: String,
        nipper_base: String,
    ) -> Self {
        Self {
            repo,
            run_repo,
            clock: Arc::new(RealIdClock),
            callback_url,
            nipper_base,
            tool_router: Self::tool_router(),
        }
    }

    /// Create a new harness (empty template when no definition is given).
    #[tool(description = "Create a new harness (empty template by default). \
        Returns the new harness_id, version_no (always 1) and lock_version.")]
    async fn sugo_create_harness(
        &self,
        Parameters(args): Parameters<tools::CreateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::usecase::create_harness::{CreateHarnessInput, create_harness};

        let out = create_harness(
            self.repo.as_ref(),
            self.clock.as_ref(),
            CreateHarnessInput { name: args.name, description: args.description, definition: args.definition },
        )
        .await
        .map_err(error::to_tool_error)?;

        let payload = serde_json::json!({
            "harness_id": out.harness_id,
            "version_no": out.version_no,
            "lock_version": out.lock_version,
        });
        Ok(CallToolResult::success(vec![Content::text(payload.to_string())]))
    }

    /// Report a harness's current status, or a summary of all harnesses.
    #[tool(description = "Get status. With harness_id: returns { harness_id, name, \
        current_version, has_draft, cells:[{id,name,status,terminal}], edges:[...], \
        draft_diff:[{cell_id,name}], \
        running_runs:[{run_id, current_cell_id, is_stalled, secs_since_last_modified}] }. \
        running_runs contains all Running-state runs with on-demand stall detection via \
        jsonl mtime (timeout 300 s). Without harness_id: returns { harnesses:[...] }.")]
    async fn sugo_status(
        &self,
        Parameters(args): Parameters<tools::StatusArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::ports::run_repository::RunRepository;
        use sugo_core::usecase::get_status::{get_status, list_harness_summaries};

        let payload = match args.harness_id {
            Some(harness_id) => {
                let st = get_status(self.repo.as_ref(), &harness_id)
                    .await
                    .map_err(error::to_tool_error)?;

                // `get_status` returns a typed `BoardDefinition`, so we project
                // directly from it: no re-parse of self-produced data.
                let def = &st.definition;

                // Project each cell from its typed fields down to the four
                // contract keys {id,name,status,terminal}. `prompt` is excluded
                // at the type level (we never read the field), so it cannot leak
                // through the status response (design L115). `status` is the
                // serde form of `CellStatus` ("active"/"draft").
                let cells: Vec<serde_json::Value> = def
                    .cells
                    .iter()
                    .map(|cell| {
                        serde_json::json!({
                            "id": cell.id,
                            "name": cell.name,
                            "status": cell.status,
                            "terminal": cell.terminal,
                        })
                    })
                    .collect();
                // Edges are emitted from the typed `Edge` values, so their shape
                // is structured by the domain types rather than passed through as
                // an untyped `Value`.
                let edges = serde_json::to_value(&def.edges).map_err(error::serde_to_tool_error)?;
                let draft_diff: Vec<serde_json::Value> = st
                    .draft_diff
                    .iter()
                    .map(|d| serde_json::json!({ "cell_id": d.cell_id, "name": d.name }))
                    .collect();

                // On-demand stall detection: list Running runs and check each
                // one's project_path via jsonl mtime (timeout 300 s).
                let all_runs = self.run_repo.list_by_harness(&harness_id)
                    .await
                    .map_err(error::to_tool_error)?;

                let running_runs: Vec<serde_json::Value> = all_runs
                    .iter()
                    .filter(|r| r.status == sugo_core::domain::run::RunStatus::Running)
                    .map(|r| {
                        let stall = r.project_path.as_deref().map(|p| {
                            sugo_infra::jsonl_watcher::check_stall(p, 300)
                        });
                        let (is_stalled, secs) = match stall {
                            Some(info) => (
                                info.is_stalled,
                                info.secs_since_last_modified
                                    .map(|s| serde_json::json!(s))
                                    .unwrap_or(serde_json::Value::Null),
                            ),
                            None => (false, serde_json::Value::Null),
                        };
                        serde_json::json!({
                            "run_id": r.id,
                            "current_cell_id": r.current_cell_id,
                            "is_stalled": is_stalled,
                            "secs_since_last_modified": secs,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "harness_id": st.harness_id,
                    "name": st.name,
                    "current_version": st.current_version,
                    "has_draft": st.has_draft,
                    "cells": cells,
                    "edges": edges,
                    "draft_diff": draft_diff,
                    "running_runs": running_runs,
                })
            }
            None => {
                let summaries = list_harness_summaries(self.repo.as_ref())
                    .await
                    .map_err(error::to_tool_error)?;
                let harnesses: Vec<serde_json::Value> = summaries
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "harness_id": s.harness_id,
                            "name": s.name,
                            "description": s.description,
                            "current_version": s.current_version,
                            "has_draft": s.has_draft,
                        })
                    })
                    .collect();
                serde_json::json!({ "harnesses": harnesses })
            }
        };
        Ok(CallToolResult::success(vec![Content::text(payload.to_string())]))
    }

    /// Replace a cell's prompt, producing a new immutable board version.
    #[tool(description = "Edit a cell's prompt. Generates a new board version and bumps the \
        optimistic lock. expected_lock_version must match the current lock or a \
        lock_conflict error is returned. Returns the new_version and lock_version.")]
    async fn sugo_edit_cell(
        &self,
        Parameters(args): Parameters<tools::EditArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::usecase::edit_cell::{EditCellInput, edit_cell};

        let out = edit_cell(
            self.repo.as_ref(),
            self.clock.as_ref(),
            EditCellInput {
                harness_id: args.harness_id,
                cell_id: args.cell_id,
                prompt: args.prompt,
                expected_lock_version: args.expected_lock_version,
            },
        )
        .await
        .map_err(error::to_tool_error)?;

        let payload = serde_json::json!({
            "harness_id": out.harness_id,
            "new_version": out.new_version,
            "lock_version": out.lock_version,
        });
        Ok(CallToolResult::success(vec![Content::text(payload.to_string())]))
    }

    /// Validate a harness's board structure and return any issues.
    #[tool(description = "Validate board structure. Pass harness_id to validate a stored \
        harness, or definition to validate a board definition directly (exactly one). \
        Returns { ok, issues } where each issue has severity, code, message and an \
        optional cell_id.")]
    async fn sugo_validate_harness(
        &self,
        Parameters(args): Parameters<tools::ValidateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::usecase::validate_harness::validate_harness;
        use sugo_core::validate::validate_board;

        // Boundary input validation: exactly one of harness_id / definition.
        let report = match (args.harness_id, args.definition) {
            (Some(_), Some(_)) => {
                return Err(ErrorData::invalid_params(
                    "provide exactly one of harness_id or definition, not both",
                    Some(serde_json::json!({ "code": "invalid_arguments" })),
                ));
            }
            (None, None) => {
                return Err(ErrorData::invalid_params(
                    "one of harness_id or definition is required",
                    Some(serde_json::json!({ "code": "invalid_arguments" })),
                ));
            }
            (Some(harness_id), None) => validate_harness(self.repo.as_ref(), &harness_id)
                .await
                .map_err(error::to_tool_error)?,
            (None, Some(def)) => validate_board(&def),
        };

        let json = serde_json::to_string(&report).map_err(error::serde_to_tool_error)?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    /// Start a harness run and inject the first cell's prompt into the Nipper message queue.
    #[tool(description = "Start a harness run. Fails with draft_cells_exist if any cell is a \
        draft. Injects the first cell's prompt (with available edges and run_id) into the \
        Nipper message queue as the next user turn. Returns { run_id } only — the prompt and \
        edges arrive exclusively via Nipper; do NOT act on them in this turn.")]
    async fn sugo_start(
        &self,
        Parameters(args): Parameters<tools::StartArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::usecase::start_run::{StartRunInput, start_run};

        let project_path = args.project_path.trim().to_string();
        if project_path.is_empty() {
            return Err(ErrorData::invalid_params(
                "project_path must be a non-empty absolute path".to_string(),
                Some(serde_json::json!({ "code": "invalid_arguments" })),
            ));
        }

        let out = start_run(
            self.repo.as_ref(),
            self.run_repo.as_ref(),
            self.clock.as_ref(),
            StartRunInput { harness_id: args.harness_id, project_path: Some(project_path.clone()) },
        )
        .await
        .map_err(error::to_tool_error)?;

        // Attach this run to the live Nipper chat session, then inject the first prompt.
        let att = nipper_client::attach(&self.nipper_base, &project_path, &out.run_id, &self.callback_url).await;
        if let Some(e) = error::nipper_outcome_error(att) {
            return Err(e);
        }
        // Build the inject text: prompt + routing footer (run_id + edges).
        // Prompt and edges are intentionally NOT returned in the MCP response so that
        // Claude must wait for the Nipper-injected turn before acting on them.
        let inject_text = build_inject_text(&out.prompt, &out.run_id, &out.edges, out.edges.is_empty());
        // Mark inject pending before calling inject to avoid a race where Nipper's
        // inject-ack arrives before set_inject_pending, leaving pending stuck forever.
        let _ = self.run_repo.set_inject_pending(&out.run_id, Some(&self.clock.now_iso())).await;
        let inj = nipper_client::inject(&self.nipper_base, &project_path, &inject_text).await;
        if let Some(e) = error::nipper_outcome_error(inj) {
            let _ = self.run_repo.set_inject_pending(&out.run_id, None).await;
            return Err(e);
        }

        let payload = serde_json::json!({ "run_id": out.run_id });
        Ok(CallToolResult::success(vec![Content::text(payload.to_string())]))
    }

    /// Advance a run along the given edge label and inject the next cell's prompt into Nipper.
    #[tool(description = "Advance a run along edge_label from the current cell. Injects the \
        next cell's prompt (with available edges and run_id) into the Nipper message queue as \
        the next user turn. Returns { ok, terminal } only — the next prompt and edges arrive \
        exclusively via Nipper; do NOT act on them in this turn. terminal=true means the run \
        is complete. Blocked with inject_pending if Nipper has not yet delivered the previous \
        inject.")]
    async fn sugo_advance(
        &self,
        Parameters(args): Parameters<tools::AdvanceArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::usecase::advance_run::{AdvanceRunInput, advance_run};

        // Inject gate: block if previous inject has not yet been acknowledged by Nipper.
        // After 30 s without an ack the inject is considered lost; mark the run Stalled
        // so the caller gets a hard error instead of being blocked forever.
        const INJECT_TIMEOUT_SECS: i64 = 30;
        if let Ok(Some(mut run)) = self.run_repo.get(&args.run_id).await {
            if let Some(ref pending_since) = run.inject_pending_since {
                let elapsed = chrono::DateTime::parse_from_rfc3339(pending_since)
                    .ok()
                    .map(|t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_seconds())
                    .unwrap_or(0)
                    .max(0);
                if elapsed < INJECT_TIMEOUT_SECS {
                    return Err(ErrorData::invalid_params(
                        "inject pending: Nipper has not yet acknowledged the previous inject; retry after /inject-ack",
                        Some(serde_json::json!({ "code": "inject_pending" })),
                    ));
                }
                // Timeout: ack never arrived — mark run Stalled and fail hard.
                run.status = sugo_core::domain::run::RunStatus::Stalled;
                run.updated_at = self.clock.now_iso();
                let _ = self.run_repo.update(&run).await;
                return Err(ErrorData::invalid_params(
                    format!(
                        "inject timeout: Nipper did not acknowledge the inject after {elapsed}s. \
                         The run is now Stalled. Call sugo_start to begin a new run."
                    ),
                    Some(serde_json::json!({ "code": "inject_timeout" })),
                ));
            }
        }

        let run_id_for_lookup = args.run_id.clone();
        let out = advance_run(
            self.repo.as_ref(),
            self.run_repo.as_ref(),
            self.clock.as_ref(),
            AdvanceRunInput { run_id: args.run_id, edge_label: args.edge_label },
        )
        .await
        .map_err(error::to_tool_error)?;

        // Inject the next cell's prompt into the attached Nipper session.
        // Prompt and edges are intentionally NOT returned in the MCP response so that
        // Claude must wait for the Nipper-injected turn before acting on them.
        if let Ok(Some(run)) = self.run_repo.get(&run_id_for_lookup).await
            && let Some(pp) = run.project_path.as_deref()
        {
            let inject_text = build_inject_text(&out.prompt, &run_id_for_lookup, &out.edges, out.terminal);
            // Mark inject pending before calling inject to avoid a race where Nipper's
            // inject-ack arrives before set_inject_pending, leaving pending stuck forever.
            if !out.terminal {
                let _ = self.run_repo.set_inject_pending(&run_id_for_lookup, Some(&self.clock.now_iso())).await;
            }
            let inj = nipper_client::inject(&self.nipper_base, pp, &inject_text).await;
            if let Some(e) = error::nipper_outcome_error(inj) {
                let _ = self.run_repo.set_inject_pending(&run_id_for_lookup, None).await;
                return Err(e);
            }
            if out.terminal {
                let _ = nipper_client::detach(&self.nipper_base, pp).await;
            }
        }

        let payload = serde_json::json!({
            "ok": true,
            "terminal": out.terminal,
        });
        Ok(CallToolResult::success(vec![Content::text(payload.to_string())]))
    }

    /// Batch-update cells and edges in one new board version.
    #[tool(description = "Batch-update a harness in a single new board version. \
        cell_changes: [{cell_id, prompt?, status?}] — prompt and status are optional \
        (omit to keep current; status must be 'active' or 'draft'). \
        edge_add: [{from, to, label, guard?}] — edges to add. \
        edge_remove: [{from, to, label}] — edges to remove (missing edges silently ignored). \
        All three arrays default to empty. Returns { harness_id, new_version, lock_version }.")]
    async fn sugo_update_harness(
        &self,
        Parameters(args): Parameters<tools::UpdateArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::domain::cell::CellStatus;
        use sugo_core::domain::edge::{Edge, Guard};
        use sugo_core::usecase::update_harness::{
            CellChange, EdgeKey, UpdateHarnessInput, update_harness,
        };

        let cell_changes: Vec<CellChange> = args
            .cell_changes
            .into_iter()
            .map(|c| {
                let status = match c.status.as_deref() {
                    None => Ok(None),
                    Some("active") => Ok(Some(CellStatus::Active)),
                    Some("draft") => Ok(Some(CellStatus::Draft)),
                    Some(other) => Err(ErrorData::invalid_params(
                        format!("unknown status '{}': must be 'active' or 'draft'", other),
                        Some(serde_json::json!({ "code": "invalid_arguments" })),
                    )),
                }?;
                Ok(CellChange { cell_id: c.cell_id, prompt: c.prompt, status })
            })
            .collect::<Result<_, ErrorData>>()?;

        let edge_add: Vec<Edge> = args
            .edge_add
            .into_iter()
            .map(|e| Edge {
                from: e.from,
                to: e.to,
                label: e.label,
                guard: e.guard.map(|expr| Guard { expr }),
            })
            .collect();

        let edge_remove: Vec<EdgeKey> = args
            .edge_remove
            .into_iter()
            .map(|k| EdgeKey { from: k.from, to: k.to, label: k.label })
            .collect();

        let out = update_harness(
            self.repo.as_ref(),
            self.clock.as_ref(),
            UpdateHarnessInput {
                harness_id: args.harness_id,
                expected_lock_version: args.expected_lock_version,
                cell_changes,
                edge_add,
                edge_remove,
            },
        )
        .await
        .map_err(error::to_tool_error)?;

        let payload = serde_json::json!({
            "harness_id": out.harness_id,
            "new_version": out.new_version,
            "lock_version": out.lock_version,
        });
        Ok(CallToolResult::success(vec![Content::text(payload.to_string())]))
    }

    /// Update a harness's description (metadata only; no new board version is created).
    #[tool(description = "Set or clear a harness's description. Pass description=null to clear. \
        Returns { ok: true }.")]
    async fn sugo_set_description(
        &self,
        Parameters(args): Parameters<tools::SetDescriptionArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        self.repo
            .set_description(&args.harness_id, args.description.as_deref())
            .await
            .map_err(error::to_tool_error)?;
        let payload = serde_json::json!({ "ok": true });
        Ok(CallToolResult::success(vec![Content::text(payload.to_string())]))
    }
}

/// Build the text injected into Nipper for each cell turn.
///
/// The prompt is the cell's instruction. A routing footer is appended so
/// Claude knows which run_id and edge labels to use — these are the ONLY
/// way Claude receives this information; they are never returned in MCP
/// responses, forcing Claude to wait for the Nipper-injected turn.
fn build_inject_text(
    prompt: &str,
    run_id: &str,
    edges: &[sugo_core::usecase::start_run::EdgeInfo],
    terminal: bool,
) -> String {
    if terminal {
        format!(
            "{}\n\n---\n【Sugo ハーネス】このセルは終端です。ハーネスの実行が完了しました。\nrun_id: {}",
            prompt, run_id
        )
    } else {
        let edge_lines: Vec<String> = edges
            .iter()
            .map(|e| match &e.guard {
                Some(g) => format!("  - \"{}\" (条件: {}) → {}", e.label, g, e.to_cell_name),
                None => format!("  - \"{}\" → {}", e.label, e.to_cell_name),
            })
            .collect();
        format!(
            "{}\n\n---\n【Sugo ハーネス】このターンのタスクが完了したら sugo_advance を呼んでください。\nrun_id: {}\n選択できるエッジ:\n{}",
            prompt, run_id, edge_lines.join("\n")
        )
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SugoServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
        info.instructions = Some(
            "Sugo harness MCP server. Tools: sugo_create_harness, sugo_status, \
             sugo_edit_cell, sugo_validate_harness, sugo_start, sugo_advance, sugo_update_harness. \
             Editing a cell always produces a new immutable board version guarded by an optimistic lock. \
             sugo_start begins a run and injects the first cell's prompt into the Nipper message queue; \
             sugo_advance follows an edge and injects the next cell's prompt into Nipper. \
             IMPORTANT: sugo_start and sugo_advance return NO prompt or edges in their MCP response. \
             The prompt, available edges, and run_id arrive exclusively as the next Nipper-injected \
             message. Claude must wait for that message before acting — never process harness content \
             in the same turn as a sugo_start/sugo_advance call."
                .to_string(),
        );
        info
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_path = match std::env::var("SUGO_DB") {
        Ok(p) => p,
        Err(_) => sugo_infra::paths::default_db_path()?
            .to_string_lossy()
            .into_owned(),
    };
    let harness_repo = Arc::new(SqliteHarnessRepository::open(&db_path)?);
    // SqliteRunRepository shares the same DB file via a separate connection.
    // The schema (including `runs` table) is applied by SqliteHarnessRepository::open.
    let run_conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| anyhow::anyhow!("open run_repo DB: {e}"))?;
    let run_repo = Arc::new(SqliteRunRepository::new(std::sync::Mutex::new(run_conn)));

    let nipper_base = nipper_client::NIPPER_BASE_URL.to_string();
    // Start the per-process callback server on an ephemeral port.
    let callback_state = callback::CallbackState {
        run_repo: run_repo.clone(),
        clock: Arc::new(RealIdClock),
        nipper_base: nipper_base.clone(),
    };
    let callback_url = callback::start(callback_state).await?;

    let server = SugoServer::new(harness_repo, run_repo, callback_url, nipper_base);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Handler-boundary tests: build a real [`SugoServer`] over an in-memory
    //! SQLite repository and drive each tool handler directly, asserting on the
    //! serialized payload shapes and on the structured error codes. These cover
    //! the MCP I/O contract (cell key projection, exclusivity rules, response
    //! envelopes) that pure argument-parsing unit tests cannot reach.

    use super::*;
    use sugo_core::domain::board::BoardDefinition;
    use sugo_core::domain::cell::{Cell, CellStatus};

    /// Build a server backed by a fresh in-memory database.
    fn server() -> SugoServer {
        let harness_repo = Arc::new(SqliteHarnessRepository::in_memory().expect("in-memory db"));
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory conn");
        conn.execute_batch(sugo_infra::sqlite::schema::SCHEMA).expect("schema");
        let run_repo = Arc::new(SqliteRunRepository::new(std::sync::Mutex::new(conn)));
        SugoServer::new(
            harness_repo,
            run_repo,
            "http://127.0.0.1:1".to_string(),
            "http://127.0.0.1:1".to_string(),
        )
    }

    /// Extract the single text payload from a successful tool result as JSON.
    fn payload(result: &CallToolResult) -> serde_json::Value {
        let text = result
            .content
            .first()
            .expect("at least one content item")
            .as_text()
            .expect("text content")
            .text
            .clone();
        serde_json::from_str(&text).expect("payload is valid JSON")
    }

    /// Read the structured `code` from an error response's `data` field.
    fn error_code(e: &ErrorData) -> String {
        e.data
            .as_ref()
            .expect("data present")
            .get("code")
            .expect("code key")
            .as_str()
            .expect("code is string")
            .to_string()
    }

    /// A minimal valid board: one active terminal cell named `start`.
    fn valid_board() -> BoardDefinition {
        BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "start".into(),
                prompt: "p".into(),
                status: CellStatus::Active,
                terminal: true,
            }],
            edges: vec![],
        }
    }

    /// Create a harness through the tool handler and return its id.
    async fn create_harness(srv: &SugoServer, name: &str, def: Option<BoardDefinition>) -> String {
        let result = srv
            .sugo_create_harness(Parameters(tools::CreateArgs {
                name: name.into(),
                description: None,
                definition: def,
            }))
            .await
            .expect("create succeeds");
        payload(&result)["harness_id"]
            .as_str()
            .expect("harness_id string")
            .to_string()
    }

    #[tokio::test]
    async fn validate_rejects_both_inputs() {
        // (Some, Some): supplying both harness_id and definition is invalid.
        let srv = server();
        let err = srv
            .sugo_validate_harness(Parameters(tools::ValidateArgs {
                harness_id: Some("h1".into()),
                definition: Some(valid_board()),
            }))
            .await
            .expect_err("both inputs must be rejected");
        assert_eq!(error_code(&err), "invalid_arguments");
    }

    #[tokio::test]
    async fn validate_rejects_no_inputs() {
        // (None, None): neither input supplied is invalid.
        let srv = server();
        let err = srv
            .sugo_validate_harness(Parameters(tools::ValidateArgs {
                harness_id: None,
                definition: None,
            }))
            .await
            .expect_err("missing inputs must be rejected");
        assert_eq!(error_code(&err), "invalid_arguments");
    }

    #[tokio::test]
    async fn validate_by_harness_id_returns_report() {
        // harness_id alone validates the stored harness via the DB.
        let srv = server();
        let id = create_harness(&srv, "h", Some(valid_board())).await;
        let result = srv
            .sugo_validate_harness(Parameters(tools::ValidateArgs {
                harness_id: Some(id),
                definition: None,
            }))
            .await
            .expect("validate succeeds");
        let p = payload(&result);
        assert_eq!(p["ok"], serde_json::json!(true));
        assert!(p["issues"].is_array());
    }

    #[tokio::test]
    async fn validate_by_definition_is_db_independent() {
        // definition alone validates directly without any stored harness, and an
        // invalid board surfaces an error issue with ok == false.
        let srv = server();
        let bad = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "c1".into(),
                prompt: "p".into(),
                status: CellStatus::Active,
                terminal: false, // no terminal cell -> error issue
            }],
            edges: vec![],
        };
        let result = srv
            .sugo_validate_harness(Parameters(tools::ValidateArgs {
                harness_id: None,
                definition: Some(bad),
            }))
            .await
            .expect("validate succeeds");
        let p = payload(&result);
        assert_eq!(p["ok"], serde_json::json!(false));
        let issues = p["issues"].as_array().expect("issues array");
        assert!(issues
            .iter()
            .any(|i| i["severity"] == serde_json::json!("error")));
    }

    #[tokio::test]
    async fn status_detail_projects_cells_to_four_keys() {
        // Detail mode must return cells with exactly {id,name,status,terminal}
        // and never leak the `prompt` field. edges and draft_diff are top-level.
        let srv = server();
        let id = create_harness(&srv, "h", Some(valid_board())).await;
        let result = srv
            .sugo_status(Parameters(tools::StatusArgs { harness_id: Some(id) }))
            .await
            .expect("status succeeds");
        let p = payload(&result);

        let cells = p["cells"].as_array().expect("cells array");
        assert_eq!(cells.len(), 1);
        let cell = cells[0].as_object().expect("cell object");
        let mut keys: Vec<&String> = cell.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["id", "name", "status", "terminal"]);
        assert!(!cell.contains_key("prompt"), "prompt must not leak");

        assert!(p["edges"].is_array(), "edges is a top-level array");
        assert!(p["draft_diff"].is_array(), "draft_diff is a top-level array");
    }

    #[tokio::test]
    async fn status_detail_includes_running_runs() {
        // Detail mode must include running_runs as an empty array when no runs exist.
        let srv = server();
        let id = create_harness(&srv, "h", Some(valid_board())).await;
        let result = srv
            .sugo_status(Parameters(tools::StatusArgs { harness_id: Some(id) }))
            .await
            .expect("status succeeds");
        let p = payload(&result);
        // Initially no runs, so running_runs should be an empty array
        assert!(p["running_runs"].is_array(), "running_runs must be an array");
        assert_eq!(p["running_runs"].as_array().unwrap().len(), 0, "no runs yet");
    }

    #[tokio::test]
    async fn status_summary_returns_harness_list() {
        // Without a harness_id, status returns the all-harnesses summary shape.
        let srv = server();
        create_harness(&srv, "a", None).await;
        let result = srv
            .sugo_status(Parameters(tools::StatusArgs { harness_id: None }))
            .await
            .expect("status succeeds");
        let p = payload(&result);
        let harnesses = p["harnesses"].as_array().expect("harnesses array");
        assert_eq!(harnesses.len(), 1);
        assert!(harnesses[0]["harness_id"].is_string(), "harness_id must be a string");
        assert_eq!(harnesses[0]["name"], serde_json::json!("a"), "name must be 'a'");
        assert_eq!(harnesses[0]["current_version"], serde_json::json!(1), "initial version_no is 1");
        assert_eq!(harnesses[0]["has_draft"], serde_json::json!(false), "default board has no draft");
    }

    #[tokio::test]
    async fn create_harness_payload_shape() {
        // Create returns harness_id plus version_no==1 and a lock_version.
        let srv = server();
        let result = srv
            .sugo_create_harness(Parameters(tools::CreateArgs {
                name: "h".into(),
                description: None,
                definition: None,
            }))
            .await
            .expect("create succeeds");
        let p = payload(&result);
        assert!(p["harness_id"].is_string());
        assert_eq!(p["version_no"], serde_json::json!(1));
        assert!(p["lock_version"].is_number());
    }

    #[tokio::test]
    async fn edit_cell_payload_shape() {
        // Edit returns harness_id, the new_version and the bumped lock_version.
        let srv = server();
        let id = create_harness(&srv, "h", Some(valid_board())).await;
        let result = srv
            .sugo_edit_cell(Parameters(tools::EditArgs {
                harness_id: id.clone(),
                cell_id: "c1".into(),
                prompt: "updated".into(),
                expected_lock_version: 0,
            }))
            .await
            .expect("edit succeeds");
        let p = payload(&result);
        assert_eq!(p["harness_id"], serde_json::json!(id));
        assert_eq!(p["new_version"], serde_json::json!(2), "first edit produces version_no 2");
        assert_eq!(p["lock_version"], serde_json::json!(1), "first edit bumps lock_version to 1");
    }

    #[tokio::test]
    async fn sugo_start_empty_project_path_is_invalid() {
        // project_path is required and must be non-empty/non-whitespace.
        let srv = server();
        let id = create_harness(&srv, "h", Some(valid_board())).await;
        let err = srv
            .sugo_start(Parameters(tools::StartArgs { harness_id: id, project_path: "  ".into() }))
            .await
            .expect_err("blank project_path must be rejected");
        assert_eq!(error_code(&err), "invalid_arguments");
    }

    #[tokio::test]
    async fn sugo_start_attach_unreachable_without_nipper() {
        // The run is created in core, then attach to the (absent) Nipper fails,
        // surfacing nipper_unreachable. Happy-path injection is covered by manual E2E.
        use sugo_core::ports::run_repository::RunRepository;
        let srv = server();
        let id = create_harness(&srv, "h", Some(valid_board())).await;
        let err = srv
            .sugo_start(Parameters(tools::StartArgs { harness_id: id.clone(), project_path: "/abs/p".into() }))
            .await
            .expect_err("attach to absent Nipper must fail");
        assert_eq!(error_code(&err), "nipper_unreachable");
        // The run was still created before the attach attempt.
        let runs = srv.run_repo.list_by_harness(&id).await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].project_path.as_deref(), Some("/abs/p"));
    }

    #[tokio::test]
    async fn sugo_start_rejects_draft_harness() {
        let srv = server();
        let draft_board = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "c1".into(),
                    prompt: "p".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c2".into(),
                    name: "draft".into(),
                    prompt: "".into(),
                    status: CellStatus::Draft,
                    terminal: false,
                },
            ],
            edges: vec![],
        };
        let id = create_harness(&srv, "h", Some(draft_board)).await;
        let err = srv
            .sugo_start(Parameters(tools::StartArgs { harness_id: id, project_path: "/abs/p".into() }))
            .await
            .expect_err("draft harness must be rejected");
        assert_eq!(error_code(&err), "draft_cells_exist");
    }

    /// Seed a run directly via the core start_run usecase (bypassing the HTTP
    /// attach/inject the MCP handler performs), returning the run_id. Used by
    /// advance tests so they don't depend on a live Nipper.
    async fn seed_run(srv: &SugoServer, harness_id: &str) -> String {
        use sugo_core::usecase::start_run::{StartRunInput, start_run};
        let out = start_run(
            srv.repo.as_ref(),
            srv.run_repo.as_ref(),
            srv.clock.as_ref(),
            StartRunInput { harness_id: harness_id.into(), project_path: Some("/abs/p".into()) },
        )
        .await
        .expect("seed start_run");
        out.run_id
    }

    #[tokio::test]
    async fn sugo_advance_moves_to_next_cell_and_marks_done() {
        use sugo_core::domain::edge::Edge;
        let srv = server();
        let two_cell = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "first".into(),
                    prompt: "do first".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c2".into(),
                    name: "last".into(),
                    prompt: "done".into(),
                    status: CellStatus::Active,
                    terminal: true,
                },
            ],
            edges: vec![Edge {
                from: "c1".into(),
                to: "c2".into(),
                label: "next".into(),
                guard: None,
            }],
        };
        use sugo_core::ports::run_repository::RunRepository;
        let id = create_harness(&srv, "h", Some(two_cell)).await;
        let run_id = seed_run(&srv, &id).await;
        // advance_run succeeds in core (run moves to terminal/Done), then the
        // inject to the absent Nipper fails with nipper_unreachable. The core
        // transition is still persisted before the inject attempt.
        let err = srv
            .sugo_advance(Parameters(tools::AdvanceArgs {
                run_id: run_id.clone(),
                edge_label: "next".into(),
            }))
            .await
            .expect_err("inject to absent Nipper must fail");
        assert_eq!(error_code(&err), "nipper_unreachable");
        let run = srv.run_repo.get(&run_id).await.unwrap().unwrap();
        assert_eq!(run.current_cell_id, "c2");
        assert_eq!(run.status, sugo_core::domain::run::RunStatus::Done);
    }

    #[tokio::test]
    async fn sugo_advance_inject_pending_within_timeout_blocks() {
        // inject_pending_since set to now (< 30 s) → inject_pending error.
        use sugo_core::domain::edge::Edge;
        use sugo_core::ports::run_repository::RunRepository;
        let srv = server();
        let two_cell = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell { id: "c1".into(), name: "first".into(), prompt: "p".into(), status: CellStatus::Active, terminal: false },
                Cell { id: "c2".into(), name: "last".into(), prompt: "done".into(), status: CellStatus::Active, terminal: true },
            ],
            edges: vec![Edge { from: "c1".into(), to: "c2".into(), label: "next".into(), guard: None }],
        };
        let id = create_harness(&srv, "h", Some(two_cell)).await;
        let run_id = seed_run(&srv, &id).await;
        let now_ts = chrono::Utc::now().to_rfc3339();
        let _ = srv.run_repo.set_inject_pending(&run_id, Some(&now_ts)).await;
        let err = srv
            .sugo_advance(Parameters(tools::AdvanceArgs { run_id, edge_label: "next".into() }))
            .await
            .expect_err("inject_pending must block advance");
        assert_eq!(error_code(&err), "inject_pending");
    }

    #[tokio::test]
    async fn sugo_advance_inject_timeout_marks_run_stalled() {
        // inject_pending_since set to 60 s ago (> 30 s) → inject_timeout + Stalled.
        use sugo_core::domain::edge::Edge;
        use sugo_core::domain::run::RunStatus;
        use sugo_core::ports::run_repository::RunRepository;
        let srv = server();
        let two_cell = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell { id: "c1".into(), name: "first".into(), prompt: "p".into(), status: CellStatus::Active, terminal: false },
                Cell { id: "c2".into(), name: "last".into(), prompt: "done".into(), status: CellStatus::Active, terminal: true },
            ],
            edges: vec![Edge { from: "c1".into(), to: "c2".into(), label: "next".into(), guard: None }],
        };
        let id = create_harness(&srv, "h", Some(two_cell)).await;
        let run_id = seed_run(&srv, &id).await;
        let old_ts = (chrono::Utc::now() - chrono::Duration::seconds(60)).to_rfc3339();
        let _ = srv.run_repo.set_inject_pending(&run_id, Some(&old_ts)).await;
        let err = srv
            .sugo_advance(Parameters(tools::AdvanceArgs { run_id: run_id.clone(), edge_label: "next".into() }))
            .await
            .expect_err("inject_timeout must reject");
        assert_eq!(error_code(&err), "inject_timeout");
        let run = srv.run_repo.get(&run_id).await.unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Stalled);
    }

    #[tokio::test]
    async fn sugo_advance_on_done_run_returns_run_not_running() {
        use sugo_core::domain::edge::Edge;
        let srv = server();
        let two_cell = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "first".into(),
                    prompt: "p".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c2".into(),
                    name: "last".into(),
                    prompt: "done".into(),
                    status: CellStatus::Active,
                    terminal: true,
                },
            ],
            edges: vec![Edge { from: "c1".into(), to: "c2".into(), label: "next".into(), guard: None }],
        };
        let id = create_harness(&srv, "h", Some(two_cell)).await;
        let run_id = seed_run(&srv, &id).await;
        // advance to terminal (Done). Core transition succeeds; the subsequent
        // inject to the absent Nipper errors with nipper_unreachable, but the
        // run is already Done in the DB.
        let err = srv
            .sugo_advance(Parameters(tools::AdvanceArgs {
                run_id: run_id.clone(),
                edge_label: "next".into(),
            }))
            .await
            .expect_err("inject to absent Nipper must fail");
        assert_eq!(error_code(&err), "nipper_unreachable");
        // advance again on Done run → run_not_running (core rejects before inject)
        let err = srv
            .sugo_advance(Parameters(tools::AdvanceArgs { run_id, edge_label: "next".into() }))
            .await
            .expect_err("Done run must reject");
        assert_eq!(error_code(&err), "run_not_running");
    }

    /// A board with one active non-terminal cell and one draft terminal cell.
    fn draft_board() -> BoardDefinition {
        use sugo_core::domain::edge::Edge;
        BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![
                Cell {
                    id: "c1".into(),
                    name: "start".into(),
                    prompt: "do start".into(),
                    status: CellStatus::Active,
                    terminal: false,
                },
                Cell {
                    id: "c2".into(),
                    name: "finish".into(),
                    prompt: "".into(),
                    status: CellStatus::Draft,
                    terminal: true,
                },
            ],
            edges: vec![
                Edge { from: "c1".into(), to: "c2".into(), label: "next".into(), guard: None },
            ],
        }
    }

    #[tokio::test]
    async fn update_harness_promotes_draft_and_has_draft_becomes_false() {
        let srv = server();
        let hid = create_harness(&srv, "h", Some(draft_board())).await;

        let result = srv
            .sugo_update_harness(Parameters(tools::UpdateArgs {
                harness_id: hid.clone(),
                expected_lock_version: 0,
                cell_changes: vec![tools::CellChangeArgs {
                    cell_id: "c2".into(),
                    prompt: Some("done".into()),
                    status: Some("active".into()),
                }],
                edge_add: vec![],
                edge_remove: vec![],
            }))
            .await
            .expect("update succeeds");

        let p = payload(&result);
        assert_eq!(p["harness_id"].as_str().unwrap(), hid);
        assert_eq!(p["new_version"].as_i64().unwrap(), 2);
        assert_eq!(p["lock_version"].as_i64().unwrap(), 1);

        let st = srv
            .sugo_status(Parameters(tools::StatusArgs { harness_id: Some(hid.clone()) }))
            .await
            .unwrap();
        let st_p = payload(&st);
        assert!(!st_p["has_draft"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn update_harness_then_start_passes_draft_check() {
        let srv = server();
        let hid = create_harness(&srv, "h", Some(draft_board())).await;

        srv.sugo_update_harness(Parameters(tools::UpdateArgs {
            harness_id: hid.clone(),
            expected_lock_version: 0,
            cell_changes: vec![tools::CellChangeArgs {
                cell_id: "c2".into(),
                prompt: Some("done".into()),
                status: Some("active".into()),
            }],
            edge_add: vec![],
            edge_remove: vec![],
        }))
        .await
        .expect("update succeeds");

        // After draft promotion sugo_start no longer rejects on draft_cells_exist;
        // it proceeds to attach, which fails against the absent Nipper. That the
        // error is nipper_unreachable (not draft_cells_exist) proves the draft
        // gate was cleared. Happy-path injection is covered by manual E2E.
        let err = srv
            .sugo_start(Parameters(tools::StartArgs {
                harness_id: hid.clone(),
                project_path: "/abs/p".into(),
            }))
            .await
            .expect_err("attach to absent Nipper fails");
        assert_eq!(error_code(&err), "nipper_unreachable");
    }

    #[tokio::test]
    async fn update_harness_unknown_status_returns_invalid_arguments() {
        let srv = server();
        let hid = create_harness(&srv, "h", None).await;

        let err = srv
            .sugo_update_harness(Parameters(tools::UpdateArgs {
                harness_id: hid.clone(),
                expected_lock_version: 0,
                cell_changes: vec![tools::CellChangeArgs {
                    cell_id: "start".into(),
                    prompt: None,
                    status: Some("unknown_status".into()),
                }],
                edge_add: vec![],
                edge_remove: vec![],
            }))
            .await
            .expect_err("unknown status must be rejected");

        let code = err
            .data
            .as_ref()
            .unwrap()
            .get("code")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(code, "invalid_arguments");
    }
}
