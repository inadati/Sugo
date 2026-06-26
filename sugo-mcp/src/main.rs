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
use tools::RealIdClock;

#[derive(Clone)]
struct SugoServer {
    repo: Arc<SqliteHarnessRepository>,
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
    fn new(repo: Arc<SqliteHarnessRepository>) -> Self {
        Self {
            repo,
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
        draft_diff:[{cell_id,name}] }. Without harness_id: returns { harnesses: \
        [{harness_id,name,current_version,has_draft}, ...] }.")]
    async fn sugo_status(
        &self,
        Parameters(args): Parameters<tools::StatusArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        use sugo_core::usecase::get_status::{get_status, list_harness_summaries};

        let payload = match args.harness_id {
            Some(harness_id) => {
                let st = get_status(self.repo.as_ref(), &harness_id)
                    .await
                    .map_err(error::to_tool_error)?;

                // The stored definition is a structured object; expose its cells
                // and edges as top-level keys per the design I/O contract.
                let definition: serde_json::Value =
                    serde_json::from_str(&st.definition_json).map_err(error::serde_to_tool_error)?;
                let cells = definition.get("cells").cloned().unwrap_or(serde_json::json!([]));
                let edges = definition.get("edges").cloned().unwrap_or(serde_json::json!([]));
                let draft_diff: Vec<serde_json::Value> = st
                    .draft_diff
                    .iter()
                    .map(|d| serde_json::json!({ "cell_id": d.cell_id, "name": d.name }))
                    .collect();

                serde_json::json!({
                    "harness_id": st.harness_id,
                    "name": st.name,
                    "current_version": st.current_version,
                    "has_draft": st.has_draft,
                    "cells": cells,
                    "edges": edges,
                    "draft_diff": draft_diff,
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
        use sugo_core::validate::validate_definition;

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
            (None, Some(def)) => validate_definition(&def),
        };

        let json = serde_json::to_string(&report).map_err(error::serde_to_tool_error)?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SugoServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
        info.instructions = Some(
            "Sugo harness MCP server. Tools: sugo_create_harness, sugo_status, \
             sugo_edit_cell, sugo_validate_harness. Editing a cell always produces a \
             new immutable board version guarded by an optimistic lock."
                .to_string(),
        );
        info
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_path = std::env::var("SUGO_DB").unwrap_or_else(|_| "sugo.db".into());
    let repo = Arc::new(SqliteHarnessRepository::open(&db_path)?);
    let server = SugoServer::new(repo);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
