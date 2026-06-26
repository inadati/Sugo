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
    use crate::ports::repository::fake::{FakeIdClock, InMemoryHarnessRepository};
    use crate::usecase::create_harness::{CreateHarnessInput, create_harness};

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
}
