//! Sugo MCP server: exposes the four P1 harness tools over rmcp/stdio.
//!
//! Each tool is a thin adapter that parses its arguments, calls the matching
//! `sugo-core` use case against a `SqliteHarnessRepository`, and serialises the
//! result. Domain errors are mapped to tool errors via [`error::to_tool_error`].

mod error;
mod tools;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router, transport::stdio};
use std::sync::Arc;
use sugo_infra::sqlite::SqliteHarnessRepository;
use sugo_infra::sqlite::SqliteRunRepository;
use tools::RealIdClock;

#[derive(Clone)]
struct SugoServer {
    repo: Arc<SqliteHarnessRepository>,
    run_repo: Arc<SqliteRunRepository>,
    clock: Arc<RealIdClock>,
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for SugoServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SugoServer").finish()
    }
}

#[tool_router]
impl SugoServer {
    fn new(repo: Arc<SqliteHarnessRepository>, run_repo: Arc<SqliteRunRepository>) -> Self {
        Self {
            repo,
            run_repo,
            clock: Arc::new(RealIdClock),
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
            CreateHarnessInput { name: args.name, definition: args.definition },
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

    /// Start a harness run and return the first cell's prompt and outgoing edges.
    #[tool(description = "Start a harness run. Fails with draft_cells_exist if any cell is a \
        draft. Returns { run_id, cell_name, prompt, edges: [{label, to_cell_id, to_cell_name, \
        guard?}] }.")]
    async fn sugo_start(
        &self,
        Parameters(args): Parameters<tools::StartArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::usecase::start_run::{StartRunInput, start_run};

        let out = start_run(
            self.repo.as_ref(),
            self.run_repo.as_ref(),
            self.clock.as_ref(),
            StartRunInput { harness_id: args.harness_id, project_path: args.project_path },
        )
        .await
        .map_err(error::to_tool_error)?;

        let edges: Vec<serde_json::Value> = out.edges.iter().map(|e| {
            let mut obj = serde_json::json!({
                "label": e.label,
                "to_cell_id": e.to_cell_id,
                "to_cell_name": e.to_cell_name,
            });
            if let Some(g) = &e.guard {
                obj["guard"] = serde_json::json!(g);
            }
            obj
        }).collect();

        let payload = serde_json::json!({
            "run_id": out.run_id,
            "cell_name": out.cell_name,
            "prompt": out.prompt,
            "edges": edges,
        });
        Ok(CallToolResult::success(vec![Content::text(payload.to_string())]))
    }

    /// Advance a run along the given edge label to the next cell.
    #[tool(description = "Advance a run along edge_label from the current cell. Returns \
        { cell_name, prompt, terminal, edges: [{label, to_cell_id, to_cell_name, guard?}] }. \
        terminal=true means the run is complete.")]
    async fn sugo_advance(
        &self,
        Parameters(args): Parameters<tools::AdvanceArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::usecase::advance_run::{AdvanceRunInput, advance_run};

        let out = advance_run(
            self.repo.as_ref(),
            self.run_repo.as_ref(),
            self.clock.as_ref(),
            AdvanceRunInput { run_id: args.run_id, edge_label: args.edge_label },
        )
        .await
        .map_err(error::to_tool_error)?;

        let edges: Vec<serde_json::Value> = out.edges.iter().map(|e| {
            let mut obj = serde_json::json!({
                "label": e.label,
                "to_cell_id": e.to_cell_id,
                "to_cell_name": e.to_cell_name,
            });
            if let Some(g) = &e.guard {
                obj["guard"] = serde_json::json!(g);
            }
            obj
        }).collect();

        let payload = serde_json::json!({
            "cell_name": out.cell_name,
            "prompt": out.prompt,
            "terminal": out.terminal,
            "edges": edges,
        });
        Ok(CallToolResult::success(vec![Content::text(payload.to_string())]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SugoServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
        info.instructions = Some(
            "Sugo harness MCP server. Tools: sugo_create_harness, sugo_status, \
             sugo_edit_cell, sugo_validate_harness, sugo_start, sugo_advance. \
             Editing a cell always produces a new immutable board version guarded \
             by an optimistic lock. sugo_start begins a run and returns the first \
             cell's prompt; sugo_advance follows an edge to the next cell."
                .to_string(),
        );
        info
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_path = std::env::var("SUGO_DB").unwrap_or_else(|_| "sugo.db".into());
    let harness_repo = Arc::new(SqliteHarnessRepository::open(&db_path)?);
    // SqliteRunRepository shares the same DB file via a separate connection.
    // The schema (including `runs` table) is applied by SqliteHarnessRepository::open.
    let run_conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| anyhow::anyhow!("open run_repo DB: {e}"))?;
    let run_repo = Arc::new(SqliteRunRepository::new(std::sync::Mutex::new(run_conn)));
    let server = SugoServer::new(harness_repo, run_repo);
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
        SugoServer::new(harness_repo, run_repo)
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
    async fn sugo_start_returns_run_and_prompt() {
        let srv = server();
        let id = create_harness(&srv, "h", Some(valid_board())).await;
        let result = srv
            .sugo_start(Parameters(tools::StartArgs { harness_id: id, project_path: None }))
            .await
            .expect("start succeeds");
        let p = payload(&result);
        assert!(p["run_id"].is_string());
        assert_eq!(p["cell_name"], serde_json::json!("start"));
        assert!(p["prompt"].is_string());
        assert!(p["edges"].is_array());
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
            .sugo_start(Parameters(tools::StartArgs { harness_id: id, project_path: None }))
            .await
            .expect_err("draft harness must be rejected");
        assert_eq!(error_code(&err), "draft_cells_exist");
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
        let id = create_harness(&srv, "h", Some(two_cell)).await;
        let start_res = srv
            .sugo_start(Parameters(tools::StartArgs { harness_id: id, project_path: None }))
            .await
            .unwrap();
        let run_id = payload(&start_res)["run_id"].as_str().unwrap().to_string();
        let adv = srv
            .sugo_advance(Parameters(tools::AdvanceArgs {
                run_id,
                edge_label: "next".into(),
            }))
            .await
            .unwrap();
        let p = payload(&adv);
        assert_eq!(p["cell_name"], serde_json::json!("last"));
        assert_eq!(p["terminal"], serde_json::json!(true));
        assert!(p["edges"].as_array().unwrap().is_empty());
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
        let start_res = srv
            .sugo_start(Parameters(tools::StartArgs { harness_id: id, project_path: None }))
            .await
            .unwrap();
        let run_id = payload(&start_res)["run_id"].as_str().unwrap().to_string();
        // advance to terminal (Done)
        srv.sugo_advance(Parameters(tools::AdvanceArgs {
            run_id: run_id.clone(),
            edge_label: "next".into(),
        }))
        .await
        .unwrap();
        // advance again on Done run → run_not_running
        let err = srv
            .sugo_advance(Parameters(tools::AdvanceArgs { run_id, edge_label: "next".into() }))
            .await
            .expect_err("Done run must reject");
        assert_eq!(error_code(&err), "run_not_running");
    }
}
