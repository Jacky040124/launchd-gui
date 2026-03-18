use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use launchpad::adapter::fs_ops::SystemFsOps;
use launchpad::adapter::fs_scan::FileScanner;
use launchpad::adapter::launchctl::{current_uid, SystemLaunchctlClient};
use launchpad::adapter::plist_reader::SystemPlistReader;
use launchpad::domain::action::TriggerAction;
use launchpad::domain::job::{JobScope, JobSummary};
use launchpad::service::action_service::ActionService;
use launchpad::service::delete_service::DeleteService;
use launchpad::service::job_service::JobService;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use tracing_subscriber::EnvFilter;

use crate::ui_state::UiState;
use crate::MainWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeFilter {
    UserAgent,
    GlobalAgent,
    SystemDaemon,
}

impl ScopeFilter {
    fn from_ui_value(value: &str) -> Option<Self> {
        match value {
            "user-agent" => Some(Self::UserAgent),
            "global-agent" => Some(Self::GlobalAgent),
            "system-daemon" => Some(Self::SystemDaemon),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::UserAgent => "user-agent",
            Self::GlobalAgent => "global-agent",
            Self::SystemDaemon => "system-daemon",
        }
    }

    fn matches(self, scope: &JobScope) -> bool {
        matches!(
            (self, scope),
            (Self::UserAgent, JobScope::UserAgent)
                | (Self::GlobalAgent, JobScope::GlobalAgent)
                | (Self::SystemDaemon, JobScope::SystemDaemon)
        )
    }
}

struct AppController {
    job_service: JobService,
    action_service: ActionService,
    delete_service: DeleteService,
    jobs: Vec<JobSummary>,
    visible_indices: Vec<usize>,
    search_query: String,
    scope_filter: Option<ScopeFilter>,
    ui_state: UiState,
}

impl AppController {
    fn new() -> Self {
        let launchctl = Arc::new(SystemLaunchctlClient);
        let plist_reader = Arc::new(SystemPlistReader);
        let fs_ops = Arc::new(SystemFsOps);
        let uid = current_uid();

        Self {
            job_service: JobService::new(
                FileScanner::new_default(),
                plist_reader,
                launchctl.clone(),
                uid,
            ),
            action_service: ActionService::new(launchctl.clone(), uid),
            delete_service: DeleteService::new(launchctl, fs_ops, uid),
            jobs: Vec::new(),
            visible_indices: Vec::new(),
            search_query: String::new(),
            scope_filter: None,
            ui_state: UiState::default(),
        }
    }

    fn refresh(&mut self, ui: &MainWindow) {
        ui.set_busy(true);
        self.ui_state.reset_pending_delete();
        ui.set_confirm_delete_visible(false);
        ui.set_confirm_delete_label("".into());
        match self.job_service.list_jobs() {
            Ok(jobs) => {
                self.jobs = jobs;

                if self.jobs.is_empty() {
                    self.ui_state.clear_selection();
                    self.visible_indices.clear();
                    self.update_filter_badge(ui);
                    ui.set_job_lines(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
                    self.update_selection_details(ui);
                    ui.set_status_message(
                        "No launchd jobs found in configured directories.".into(),
                    );
                    ui.set_busy(false);
                    return;
                }

                self.apply_filters_and_render(ui);
                let visible_count = self.visible_indices.len();
                if visible_count == 0 {
                    ui.set_status_message(
                        format!(
                            "Loaded {} jobs. No jobs match current filters.",
                            self.jobs.len()
                        )
                        .into(),
                    );
                } else {
                    ui.set_status_message(
                        format!(
                            "Loaded {} jobs ({} visible).",
                            self.jobs.len(),
                            visible_count
                        )
                        .into(),
                    );
                }
                ui.set_busy(false);
            }
            Err(err) => {
                self.jobs.clear();
                self.visible_indices.clear();
                self.ui_state.clear_selection();
                self.update_filter_badge(ui);
                ui.set_job_lines(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
                self.update_selection_details(ui);
                ui.set_status_message(format!("Failed to refresh jobs: {err}").into());
                ui.set_busy(false);
            }
        }
    }

    fn select(&mut self, ui: &MainWindow, index: usize) {
        let Some(job_index) = self.visible_indices.get(index).copied() else {
            return;
        };
        self.ui_state.select(job_index);
        ui.set_confirm_delete_visible(false);
        ui.set_confirm_delete_label("".into());
        self.update_selection_details(ui);
        ui.set_status_message(format!("Selected {}", self.jobs[job_index].label).into());
    }

    fn query_changed(&mut self, ui: &MainWindow, query: &str) {
        self.search_query = query.trim().to_string();
        self.apply_filters_and_render(ui);
        let visible_count = self.visible_indices.len();
        ui.set_status_message(
            format!(
                "Applied search filter '{}' ({} visible).",
                self.search_query, visible_count
            )
            .into(),
        );
    }

    fn scope_filter_requested(&mut self, ui: &MainWindow, value: &str) {
        self.scope_filter = ScopeFilter::from_ui_value(value);
        self.apply_filters_and_render(ui);
        let visible_count = self.visible_indices.len();
        let scope_text = self.scope_filter.map_or("all scopes", ScopeFilter::label);
        ui.set_status_message(
            format!(
                "Applied scope filter '{}' ({} visible).",
                scope_text, visible_count
            )
            .into(),
        );
    }

    fn clear_filters(&mut self, ui: &MainWindow) {
        self.scope_filter = None;
        self.search_query.clear();
        ui.set_query_text("".into());
        self.apply_filters_and_render(ui);
        ui.set_status_message("Cleared filters.".into());
    }

    fn trigger(&mut self, ui: &MainWindow, action: TriggerAction) {
        let Some(index) = self.ui_state.selected_index else {
            ui.set_status_message("Select a job first.".into());
            return;
        };
        if index >= self.jobs.len() {
            ui.set_status_message("Selected job is no longer available.".into());
            return;
        }

        self.ui_state.cancel_delete_confirmation();
        ui.set_confirm_delete_visible(false);
        ui.set_confirm_delete_label("".into());
        let selected_job = self.jobs[index].clone();
        ui.set_busy(true);
        match self.action_service.execute(&selected_job, action) {
            Ok(()) => {
                ui.set_status_message(format!("{} command sent.", action.as_str()).into());
                self.refresh(ui);
            }
            Err(err) => {
                ui.set_status_message(format!("{} failed: {err}", action.as_str()).into());
                ui.set_busy(false);
            }
        }
    }

    fn request_delete(&mut self, ui: &MainWindow) {
        let Some(index) = self.ui_state.selected_index else {
            ui.set_status_message("Select a job first.".into());
            return;
        };
        if index >= self.jobs.len() {
            ui.set_status_message("Selected job is no longer available.".into());
            return;
        }

        self.ui_state.begin_delete_confirmation(index);
        ui.set_confirm_delete_label(self.jobs[index].label.clone().into());
        ui.set_confirm_delete_visible(true);
        ui.set_status_message("Delete requested. Review the warning and confirm deletion.".into());
    }

    fn cancel_delete(&mut self, ui: &MainWindow) {
        self.ui_state.cancel_delete_confirmation();
        ui.set_confirm_delete_visible(false);
        ui.set_confirm_delete_label("".into());
        ui.set_status_message("Delete request cancelled.".into());
    }

    fn confirm_delete(&mut self, ui: &MainWindow) {
        let Some(index) = self.ui_state.take_confirmed_delete() else {
            ui.set_confirm_delete_visible(false);
            ui.set_confirm_delete_label("".into());
            ui.set_status_message("No pending delete request.".into());
            return;
        };
        if index >= self.jobs.len() {
            ui.set_confirm_delete_visible(false);
            ui.set_confirm_delete_label("".into());
            ui.set_status_message("Selected job is no longer available.".into());
            return;
        }

        let selected_job = self.jobs[index].clone();
        ui.set_confirm_delete_visible(false);
        ui.set_busy(true);
        match self.delete_service.delete(&selected_job) {
            Ok(()) => {
                ui.set_confirm_delete_label("".into());
                ui.set_status_message("Job plist deleted successfully.".into());
                self.refresh(ui);
            }
            Err(err) => {
                ui.set_confirm_delete_label("".into());
                ui.set_status_message(format!("Delete failed: {err}").into());
                ui.set_busy(false);
            }
        }
    }

    fn apply_filters_and_render(&mut self, ui: &MainWindow) {
        self.visible_indices = self
            .jobs
            .iter()
            .enumerate()
            .filter_map(|(idx, job)| {
                job_matches_filters(job, &self.search_query, self.scope_filter).then_some(idx)
            })
            .collect();

        let list_lines: Vec<SharedString> = self
            .visible_indices
            .iter()
            .map(|idx| self.jobs[*idx].list_line().into())
            .collect();
        ui.set_job_lines(ModelRc::new(VecModel::from(list_lines)));
        self.update_filter_badge(ui);

        if self.visible_indices.is_empty() {
            self.ui_state.clear_selection();
            self.update_selection_details(ui);
            return;
        }

        let selected = self
            .ui_state
            .selected_index
            .filter(|idx| self.visible_indices.contains(idx))
            .unwrap_or(self.visible_indices[0]);
        self.ui_state.select(selected);
        self.update_selection_details(ui);
    }

    fn update_filter_badge(&self, ui: &MainWindow) {
        let scope_text = self.scope_filter.map_or("all scopes", ScopeFilter::label);
        if self.search_query.is_empty() {
            ui.set_active_filter_text(scope_text.into());
        } else {
            ui.set_active_filter_text(format!("{scope_text} + '{}'", self.search_query).into());
        }
    }

    fn update_selection_details(&self, ui: &MainWindow) {
        let Some(index) = self.ui_state.selected_index else {
            clear_details(ui);
            return;
        };

        if index >= self.jobs.len() {
            clear_details(ui);
            return;
        }

        let job = &self.jobs[index];
        ui.set_detail_label(job.label.clone().into());
        ui.set_detail_scope(job.scope.to_string().into());
        ui.set_detail_status(job.status_text().into());
        ui.set_detail_path(job.path.to_string_lossy().to_string().into());
        ui.set_detail_error(
            job.error
                .as_ref()
                .map(|msg| format!("Warning: {msg}"))
                .unwrap_or_default()
                .into(),
        );
        ui.set_can_trigger(job.capabilities.can_trigger);
        ui.set_can_delete(job.capabilities.can_delete);
    }
}

fn job_matches_filters(
    job: &JobSummary,
    search_query: &str,
    scope_filter: Option<ScopeFilter>,
) -> bool {
    let scope_matches = scope_filter.is_none_or(|filter| filter.matches(&job.scope));

    let normalized_query = search_query.trim().to_ascii_lowercase();
    if normalized_query.is_empty() {
        return scope_matches;
    }

    let label_matches = job.label.to_ascii_lowercase().contains(&normalized_query);
    let path_matches = job
        .path
        .to_string_lossy()
        .to_ascii_lowercase()
        .contains(&normalized_query);
    scope_matches && (label_matches || path_matches)
}

fn clear_details(ui: &MainWindow) {
    ui.set_detail_label("".into());
    ui.set_detail_scope("".into());
    ui.set_detail_status("".into());
    ui.set_detail_path("".into());
    ui.set_detail_error("".into());
    ui.set_confirm_delete_visible(false);
    ui.set_confirm_delete_label("".into());
    ui.set_can_trigger(false);
    ui.set_can_delete(false);
}

pub fn run() -> Result<(), slint::PlatformError> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .try_init();

    let ui = MainWindow::new()?;
    let controller = Rc::new(RefCell::new(AppController::new()));

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_refresh_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().refresh(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_select_job(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                if index >= 0 {
                    controller.borrow_mut().select(&ui, index as usize);
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_trigger_requested(move |action| {
            if let Some(ui) = ui_weak.upgrade() {
                match TriggerAction::from_ui_value(action.as_str()) {
                    Some(action) => controller.borrow_mut().trigger(&ui, action),
                    None => ui.set_status_message("Unknown action requested.".into()),
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_delete_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().request_delete(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_confirm_delete(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().confirm_delete(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_cancel_delete(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().cancel_delete(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_query_changed(move |query| {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().query_changed(&ui, query.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_scope_filter_requested(move |scope| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .scope_filter_requested(&ui, scope.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_clear_filters_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().clear_filters(&ui);
            }
        });
    }

    controller.borrow_mut().refresh(&ui);
    ui.run()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use launchpad::domain::job::{JobCapabilities, JobScope, JobSummary};
    use launchpad::domain::status::JobStatus;

    use super::{job_matches_filters, ScopeFilter};

    #[test]
    fn filter_matches_by_scope() {
        let job = job_fixture(
            "com.demo.user",
            "/Users/test/Library/LaunchAgents/com.demo.user.plist",
        );
        assert!(job_matches_filters(&job, "", Some(ScopeFilter::UserAgent)));
        assert!(!job_matches_filters(
            &job,
            "",
            Some(ScopeFilter::SystemDaemon)
        ));
    }

    #[test]
    fn filter_matches_by_label_and_path_query() {
        let job = job_fixture(
            "com.demo.searchable",
            "/Users/test/Library/LaunchAgents/com.demo.searchable.plist",
        );
        assert!(job_matches_filters(&job, "searchable", None));
        assert!(job_matches_filters(&job, "launchagents", None));
        assert!(!job_matches_filters(&job, "missing-token", None));
    }

    #[test]
    fn filter_combines_scope_and_search_query() {
        let job = job_fixture(
            "com.demo.scope",
            "/Library/LaunchDaemons/com.demo.scope.plist",
        )
        .with_scope(JobScope::SystemDaemon);
        assert!(job_matches_filters(
            &job,
            "demo.scope",
            Some(ScopeFilter::SystemDaemon)
        ));
        assert!(!job_matches_filters(
            &job,
            "demo.scope",
            Some(ScopeFilter::UserAgent)
        ));
    }

    trait JobFixtureExt {
        fn with_scope(self, scope: JobScope) -> Self;
    }

    impl JobFixtureExt for JobSummary {
        fn with_scope(mut self, scope: JobScope) -> Self {
            self.scope = scope;
            self
        }
    }

    fn job_fixture(label: &str, path: &str) -> JobSummary {
        JobSummary {
            id: label.to_string(),
            label: label.to_string(),
            path: PathBuf::from(path),
            scope: JobScope::UserAgent,
            status: JobStatus::Loaded,
            capabilities: JobCapabilities {
                can_trigger: true,
                can_delete: true,
                trigger_reason: None,
                delete_reason: None,
            },
            error: None,
        }
    }
}
