use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickLaunchItem {
    pub id: String,
    pub title: String,
    pub status: String,
}

pub trait QuickLaunchProvider: Send + Sync {
    fn sync_items(&self, items: &[QuickLaunchItem]) -> AppResult<()>;
}

#[derive(Debug, Default)]
pub struct NoopQuickLaunchProvider;

impl QuickLaunchProvider for NoopQuickLaunchProvider {
    fn sync_items(&self, _items: &[QuickLaunchItem]) -> AppResult<()> {
        Err(AppError::Validation(
            "QuickLaunch menubar integration is not available on this build.".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::adapter::quicklaunch::{NoopQuickLaunchProvider, QuickLaunchProvider};

    #[test]
    fn noop_provider_returns_explicit_error() {
        let provider = NoopQuickLaunchProvider;
        let err = provider
            .sync_items(&[])
            .expect_err("should fail on noop provider");
        assert!(err
            .to_string()
            .contains("QuickLaunch menubar integration is not available"));
    }
}
