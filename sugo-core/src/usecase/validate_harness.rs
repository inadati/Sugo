use crate::error::CoreError;
use crate::ports::repository::HarnessRepository;
use crate::validate::{ValidationReport, validate_board};

pub async fn validate_harness(
    repo: &dyn HarnessRepository,
    harness_id: &str,
) -> Result<ValidationReport, CoreError> {
    let (_, v) = repo
        .get(harness_id)
        .await?
        .ok_or_else(|| CoreError::NotFound(harness_id.to_string()))?;
    Ok(validate_board(&v.definition))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::board::BoardDefinition;
    use crate::domain::cell::{Cell, CellStatus};
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};
    use crate::usecase::create_harness::{CreateHarnessInput, create_harness};
    use crate::validate::{IssueCode, Severity};

    #[tokio::test]
    async fn validates_existing_harness() {
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), definition: None },
        )
        .await
        .unwrap();
        let report = validate_harness(&repo, &out.harness_id).await.unwrap();
        assert!(report.ok); // default_board は active+terminal で妥当
    }

    #[tokio::test]
    async fn validate_missing_harness_is_not_found() {
        let repo = InMemoryHarnessRepository::new();
        let err = validate_harness(&repo, "nope").await.unwrap_err();
        assert!(matches!(err, CoreError::NotFound(_)));
    }

    #[tokio::test]
    async fn validate_surfaces_error_issue_with_ok_false() {
        // no_terminal を持つ盤面が usecase 経由で ok=false で表面化することを検証。
        let repo = InMemoryHarnessRepository::new();
        let clock = FakeIdClock::new();
        let def = BoardDefinition {
            schema_version: 1,
            start: "c1".into(),
            cells: vec![Cell {
                id: "c1".into(),
                name: "c1".into(),
                prompt: "p".into(),
                status: CellStatus::Active,
                terminal: false, // terminal が無い → no_terminal (error)
            }],
            edges: vec![],
        };
        let out = create_harness(
            &repo,
            &clock,
            CreateHarnessInput { name: "h".into(), definition: Some(def) },
        )
        .await
        .unwrap();
        let report = validate_harness(&repo, &out.harness_id).await.unwrap();
        assert!(!report.ok);
        let issue = report
            .issues
            .iter()
            .find(|i| i.code == IssueCode::NoTerminal)
            .expect("no_terminal issue");
        assert_eq!(issue.severity, Severity::Error);
    }
}
