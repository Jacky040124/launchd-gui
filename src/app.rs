use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use launchpad::adapter::clipboard::{ClipboardClient, SystemClipboardClient};
use launchpad::adapter::fs_ops::SystemFsOps;
use launchpad::adapter::fs_scan::FileScanner;
use launchpad::adapter::launchctl::{current_uid, SystemLaunchctlClient};
use launchpad::adapter::log_stream::SystemLogStreamClient;
use launchpad::adapter::plist_doc::SystemPlistDocumentStore;
use launchpad::adapter::plist_reader::SystemPlistReader;
use launchpad::adapter::star_store::JsonStarStore;
use launchpad::domain::action::TriggerAction;
use launchpad::domain::filter::AdvancedFilter;
use launchpad::domain::job::{JobScope, JobSummary};
use launchpad::domain::job_detail::JobRuntimeDetails;
use launchpad::domain::plist_document::StandardPlistDocument;
use launchpad::domain::status::JobStatus;
use launchpad::service::action_service::ActionService;
use launchpad::service::ai_service::AiService;
use launchpad::service::delete_service::DeleteService;
use launchpad::service::diagnostic_service::DiagnosticService;
use launchpad::service::job_service::JobService;
use launchpad::service::log_service::LogService;
use launchpad::service::plist_service::PlistService;
use launchpad::service::star_service::StarService;
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum EditorTarget {
    Existing { index: usize },
    New { scope: JobScope },
}

#[derive(Debug, Clone, Default)]
struct EditorState {
    label: String,
    program: String,
    program_arguments: String,
    run_at_load: bool,
    keep_alive: bool,
    start_interval: String,
    working_directory: String,
    environment_variables: String,
    xml_preview: String,
    diagnostics_text: String,
    ai_prompt: String,
    ai_response: String,
}

impl EditorState {
    fn from_document(document: &StandardPlistDocument, plist_service: &PlistService) -> Self {
        Self {
            label: document.label.clone(),
            program: document.program.clone(),
            program_arguments: document.to_program_arguments_line(),
            run_at_load: document.run_at_load,
            keep_alive: document.keep_alive,
            start_interval: document
                .start_interval
                .map(|value| value.to_string())
                .unwrap_or_default(),
            working_directory: document.working_directory.clone().unwrap_or_default(),
            environment_variables: plist_service.format_env_pairs(&document.environment_variables),
            xml_preview: String::new(),
            diagnostics_text: String::new(),
            ai_prompt: String::new(),
            ai_response: String::new(),
        }
    }
}

struct AppController {
    job_service: JobService,
    action_service: ActionService,
    delete_service: DeleteService,
    diagnostic_service: DiagnosticService,
    log_service: LogService,
    ai_service: AiService,
    plist_service: PlistService,
    star_service: StarService,
    clipboard: Arc<dyn ClipboardClient>,
    jobs: Vec<JobSummary>,
    visible_indices: Vec<usize>,
    runtime_details: HashMap<String, JobRuntimeDetails>,
    search_query: String,
    command_query: String,
    scope_filter: Option<ScopeFilter>,
    advanced_filter: AdvancedFilter,
    starred_only: bool,
    advanced_details_visible: bool,
    recent_logs_text: String,
    editor_state: EditorState,
    editor_target: Option<EditorTarget>,
    ui_state: UiState,
}

impl AppController {
    fn new() -> Self {
        let launchctl = Arc::new(SystemLaunchctlClient);
        let plist_reader = Arc::new(SystemPlistReader);
        let fs_ops = Arc::new(SystemFsOps);
        let clipboard = Arc::new(SystemClipboardClient);
        let uid = current_uid();
        let mut star_service = StarService::new(Arc::new(JsonStarStore::new_default()));
        let _ = star_service.load();

        Self {
            job_service: JobService::new(
                FileScanner::new_default(),
                plist_reader,
                launchctl.clone(),
                uid,
            ),
            action_service: ActionService::new(launchctl.clone(), uid),
            delete_service: DeleteService::new(launchctl, fs_ops, uid),
            diagnostic_service: DiagnosticService,
            log_service: LogService::new(Arc::new(SystemLogStreamClient)),
            ai_service: AiService::new(),
            plist_service: PlistService::new(Arc::new(SystemPlistDocumentStore)),
            star_service,
            clipboard,
            jobs: Vec::new(),
            visible_indices: Vec::new(),
            runtime_details: HashMap::new(),
            search_query: String::new(),
            command_query: String::new(),
            scope_filter: None,
            advanced_filter: AdvancedFilter::default(),
            starred_only: false,
            advanced_details_visible: false,
            recent_logs_text: String::new(),
            editor_state: EditorState::default(),
            editor_target: None,
            ui_state: UiState::default(),
        }
    }

    fn refresh(&mut self, ui: &MainWindow) {
        let selected_id_before_refresh = self
            .ui_state
            .selected_index
            .and_then(|idx| self.jobs.get(idx))
            .map(|job| job.id.clone());

        ui.set_busy(true);
        self.ui_state.reset_pending_delete();
        ui.set_confirm_delete_visible(false);
        ui.set_confirm_delete_label("".into());
        match self.job_service.list_jobs() {
            Ok(mut jobs) => {
                self.star_service.apply_to_jobs(&mut jobs);
                sort_jobs_by_star_then_label(&mut jobs);
                self.jobs = jobs;
                self.runtime_details.clear();

                if self.jobs.is_empty() {
                    self.ui_state.clear_selection();
                    self.visible_indices.clear();
                    self.editor_target = None;
                    self.editor_state = EditorState::default();
                    self.sync_editor_to_ui(ui);
                    self.recent_logs_text.clear();
                    ui.set_log_view_text("".into());
                    self.update_filter_badge(ui);
                    ui.set_job_lines(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
                    ui.set_selected_job_row(-1);
                    self.update_selection_details(ui);
                    ui.set_status_message(
                        "No launchd jobs found in configured directories.".into(),
                    );
                    ui.set_busy(false);
                    return;
                }

                self.apply_filters_and_render(ui, selected_id_before_refresh.as_deref());
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
                self.editor_target = None;
                self.editor_state = EditorState::default();
                self.sync_editor_to_ui(ui);
                self.recent_logs_text.clear();
                ui.set_log_view_text("".into());
                self.update_filter_badge(ui);
                ui.set_job_lines(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
                ui.set_selected_job_row(-1);
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
        self.advanced_details_visible = false;
        ui.set_advanced_detail_visible(false);
        ui.set_confirm_delete_visible(false);
        ui.set_confirm_delete_label("".into());
        self.ensure_runtime_details_loaded(ui, job_index);
        self.load_editor_for_existing_job(ui, job_index);
        self.update_selection_details(ui);
        ui.set_status_message(format!("Selected {}", self.jobs[job_index].label).into());
    }

    fn start_new_job_editor(&mut self, ui: &MainWindow, scope: JobScope) {
        let scope_label = scope.as_str().to_string();
        self.editor_target = Some(EditorTarget::New { scope });
        self.editor_state = EditorState {
            label: "com.example.new-job".to_string(),
            program: "/usr/bin/true".to_string(),
            run_at_load: true,
            keep_alive: false,
            ..EditorState::default()
        };
        self.refresh_editor_preview(ui);
        self.sync_editor_to_ui(ui);
        ui.set_status_message(format!("Preparing new {scope_label} plist draft.").into());
    }

    fn query_changed(&mut self, ui: &MainWindow, query: &str) {
        self.search_query = query.trim().to_string();
        let preferred = self
            .ui_state
            .selected_index
            .and_then(|idx| self.jobs.get(idx))
            .map(|job| job.id.clone());
        self.apply_filters_and_render(ui, preferred.as_deref());
        let visible_count = self.visible_indices.len();
        ui.set_status_message(
            format!(
                "Applied search filter '{}' ({} visible).",
                self.search_query, visible_count
            )
            .into(),
        );
    }

    fn command_query_changed(&mut self, ui: &MainWindow, query: &str) {
        self.command_query = query.to_string();
        ui.set_command_query(self.command_query.clone().into());
    }

    fn run_command_palette(&mut self, ui: &MainWindow) {
        let normalized = self.command_query.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            ui.set_status_message("Type a command first.".into());
            return;
        }

        match normalized.as_str() {
            "refresh" => self.refresh(ui),
            "start" => self.trigger(ui, TriggerAction::Start),
            "stop" => self.trigger(ui, TriggerAction::Stop),
            "kickstart" => self.trigger(ui, TriggerAction::Kickstart),
            "enable" => self.trigger(ui, TriggerAction::Enable),
            "disable" => self.trigger(ui, TriggerAction::Disable),
            "load" => self.trigger(ui, TriggerAction::Load),
            "unload" => self.trigger(ui, TriggerAction::Unload),
            "save" => self.save_editor(ui),
            "logs" => self.load_recent_logs(ui),
            "star" | "unstar" => self.toggle_star(ui),
            "new user" | "new user job" => self.start_new_job_editor(ui, JobScope::UserAgent),
            "new global" | "new global job" => self.start_new_job_editor(ui, JobScope::GlobalAgent),
            _ => {
                ui.set_status_message(
                    format!(
                        "Unknown command '{}'. Try: refresh/start/stop/enable/load/new user/save/logs",
                        normalized
                    )
                    .into(),
                );
            }
        }
    }

    fn scope_filter_requested(&mut self, ui: &MainWindow, value: &str) {
        self.scope_filter = ScopeFilter::from_ui_value(value);
        let preferred = self
            .ui_state
            .selected_index
            .and_then(|idx| self.jobs.get(idx))
            .map(|job| job.id.clone());
        self.apply_filters_and_render(ui, preferred.as_deref());
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

    fn status_filter_requested(&mut self, ui: &MainWindow, value: &str) {
        self.advanced_filter.status = match value {
            "running" => Some(JobStatus::Running),
            "loaded" => Some(JobStatus::Loaded),
            "disabled" => Some(JobStatus::Disabled),
            "unknown" => Some(JobStatus::Unknown),
            _ => None,
        };
        self.update_advanced_filter_controls(ui);
        let preferred = self
            .ui_state
            .selected_index
            .and_then(|idx| self.jobs.get(idx))
            .map(|job| job.id.clone());
        self.apply_filters_and_render(ui, preferred.as_deref());
    }

    fn cycle_disabled_filter(&mut self, ui: &MainWindow) {
        self.advanced_filter.disabled = self.advanced_filter.disabled.cycle();
        self.update_advanced_filter_controls(ui);
        self.apply_filters_and_render(ui, None);
    }

    fn cycle_run_at_load_filter(&mut self, ui: &MainWindow) {
        self.advanced_filter.run_at_load = self.advanced_filter.run_at_load.cycle();
        self.update_advanced_filter_controls(ui);
        self.apply_filters_and_render(ui, None);
    }

    fn cycle_keep_alive_filter(&mut self, ui: &MainWindow) {
        self.advanced_filter.keep_alive = self.advanced_filter.keep_alive.cycle();
        self.update_advanced_filter_controls(ui);
        self.apply_filters_and_render(ui, None);
    }

    fn cycle_error_filter(&mut self, ui: &MainWindow) {
        self.advanced_filter.has_error = self.advanced_filter.has_error.cycle();
        self.update_advanced_filter_controls(ui);
        self.apply_filters_and_render(ui, None);
    }

    fn clear_filters(&mut self, ui: &MainWindow) {
        self.scope_filter = None;
        self.starred_only = false;
        self.search_query.clear();
        self.advanced_filter.clear();
        ui.set_query_text("".into());
        ui.set_show_starred_only(false);
        self.update_advanced_filter_controls(ui);
        self.apply_filters_and_render(ui, None);
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

    fn toggle_star(&mut self, ui: &MainWindow) {
        let Some(index) = self.ui_state.selected_index else {
            ui.set_status_message("Select a job first.".into());
            return;
        };
        if index >= self.jobs.len() {
            ui.set_status_message("Selected job is no longer available.".into());
            return;
        }

        let selected_job_id = self.jobs[index].id.clone();
        let selected_label = self.jobs[index].label.clone();
        match self.star_service.toggle(&selected_label) {
            Ok(is_starred) => {
                self.star_service.apply_to_jobs(&mut self.jobs);
                sort_jobs_by_star_then_label(&mut self.jobs);
                self.apply_filters_and_render(ui, Some(selected_job_id.as_str()));
                let status = if is_starred { "starred" } else { "unstarred" };
                ui.set_status_message(format!("{} is now {}.", selected_label, status).into());
            }
            Err(err) => {
                ui.set_status_message(format!("Failed to update star: {err}").into());
            }
        }
    }

    fn copy_selected_details(&mut self, ui: &MainWindow) {
        let Some(index) = self.ui_state.selected_index else {
            ui.set_status_message("Select a job first.".into());
            return;
        };
        if index >= self.jobs.len() {
            ui.set_status_message("Selected job is no longer available.".into());
            return;
        }

        self.ensure_runtime_details_loaded(ui, index);
        let details_text = self.formatted_job_details(index);
        match self.clipboard.set_text(&details_text) {
            Ok(_) => ui.set_status_message("Copied selected job details.".into()),
            Err(err) => ui.set_status_message(format!("Copy failed: {err}").into()),
        }
    }

    fn editor_label_changed(&mut self, ui: &MainWindow, value: &str) {
        self.editor_state.label = value.trim().to_string();
        self.refresh_editor_preview(ui);
    }

    fn editor_program_changed(&mut self, ui: &MainWindow, value: &str) {
        self.editor_state.program = value.trim().to_string();
        self.refresh_editor_preview(ui);
    }

    fn editor_args_changed(&mut self, ui: &MainWindow, value: &str) {
        self.editor_state.program_arguments = value.trim().to_string();
        self.refresh_editor_preview(ui);
    }

    fn editor_working_dir_changed(&mut self, ui: &MainWindow, value: &str) {
        self.editor_state.working_directory = value.trim().to_string();
        self.refresh_editor_preview(ui);
    }

    fn editor_start_interval_changed(&mut self, ui: &MainWindow, value: &str) {
        self.editor_state.start_interval = value.trim().to_string();
        self.refresh_editor_preview(ui);
    }

    fn editor_env_changed(&mut self, ui: &MainWindow, value: &str) {
        self.editor_state.environment_variables = value.trim().to_string();
        self.refresh_editor_preview(ui);
    }

    fn ai_prompt_changed(&mut self, ui: &MainWindow, value: &str) {
        self.editor_state.ai_prompt = value.to_string();
        ui.set_ai_prompt(self.editor_state.ai_prompt.clone().into());
    }

    fn run_ai_assistant(&mut self, ui: &MainWindow) {
        if self.editor_state.ai_prompt.trim().is_empty() {
            ui.set_status_message("Please enter AI prompt first.".into());
            return;
        }
        if self.editor_state.xml_preview.trim().is_empty() {
            self.refresh_editor_preview(ui);
        }

        match self
            .ai_service
            .suggest_edit(&self.editor_state.ai_prompt, &self.editor_state.xml_preview)
        {
            Ok(response) => {
                let mut lines = vec![format!(
                    "Provider: {}\nSummary: {}",
                    response.provider, response.summary
                )];
                if !response.suggested_patch_notes.is_empty() {
                    lines.push("Suggestions:".to_string());
                    lines.extend(
                        response
                            .suggested_patch_notes
                            .into_iter()
                            .map(|item| format!("- {item}")),
                    );
                }
                self.editor_state.ai_response = lines.join("\n");
                ui.set_ai_response(self.editor_state.ai_response.clone().into());
                ui.set_status_message("AI suggestions updated.".into());
            }
            Err(err) => {
                self.editor_state.ai_response = format!("AI request failed: {err}");
                ui.set_ai_response(self.editor_state.ai_response.clone().into());
                ui.set_status_message(format!("AI request failed: {err}").into());
            }
        }
    }

    fn toggle_editor_run_at_load(&mut self, ui: &MainWindow) {
        self.editor_state.run_at_load = !self.editor_state.run_at_load;
        ui.set_editor_run_at_load(self.editor_state.run_at_load);
        self.refresh_editor_preview(ui);
    }

    fn toggle_editor_keep_alive(&mut self, ui: &MainWindow) {
        self.editor_state.keep_alive = !self.editor_state.keep_alive;
        ui.set_editor_keep_alive(self.editor_state.keep_alive);
        self.refresh_editor_preview(ui);
    }

    fn save_editor(&mut self, ui: &MainWindow) {
        let document = match self.editor_document_from_state() {
            Ok(document) => document,
            Err(err) => {
                ui.set_status_message(format!("Editor validation failed: {err}").into());
                return;
            }
        };

        let Some(target) = self.editor_target.clone() else {
            ui.set_status_message("Select a job or create a new draft first.".into());
            return;
        };

        match target {
            EditorTarget::Existing { index } => {
                if index >= self.jobs.len() {
                    ui.set_status_message("Selected job is no longer available.".into());
                    return;
                }
                let job = self.jobs[index].clone();
                match self
                    .plist_service
                    .save_existing(&job.path, &job.scope, &document)
                {
                    Ok(_) => {
                        ui.set_status_message("Saved plist changes.".into());
                        self.refresh(ui);
                    }
                    Err(err) => ui.set_status_message(format!("Save failed: {err}").into()),
                }
            }
            EditorTarget::New { scope } => match self.plist_service.create_new(scope, &document) {
                Ok(path) => {
                    ui.set_status_message(
                        format!("Created new plist at {}", path.display()).into(),
                    );
                    self.refresh(ui);
                }
                Err(err) => ui.set_status_message(format!("Create failed: {err}").into()),
            },
        }
    }

    fn set_starred_only(&mut self, ui: &MainWindow, value: bool) {
        self.starred_only = value;
        ui.set_show_starred_only(value);
        let preferred = self
            .ui_state
            .selected_index
            .and_then(|idx| self.jobs.get(idx))
            .map(|job| job.id.clone());
        self.apply_filters_and_render(ui, preferred.as_deref());
        let msg = if value {
            "Showing starred jobs only."
        } else {
            "Showing all jobs."
        };
        ui.set_status_message(msg.into());
    }

    fn toggle_advanced_details(&mut self, ui: &MainWindow) {
        self.advanced_details_visible = !self.advanced_details_visible;
        ui.set_advanced_detail_visible(self.advanced_details_visible);
    }

    fn load_recent_logs(&mut self, ui: &MainWindow) {
        let Some(index) = self.ui_state.selected_index else {
            ui.set_status_message("Select a job first.".into());
            return;
        };
        if index >= self.jobs.len() {
            ui.set_status_message("Selected job is no longer available.".into());
            return;
        }

        let label = self.jobs[index].label.clone();
        match self.log_service.recent_logs(&label, 10, 120) {
            Ok(logs) => {
                self.recent_logs_text = if logs.trim().is_empty() {
                    "No recent logs found.".to_string()
                } else {
                    logs
                };
                ui.set_log_view_text(self.recent_logs_text.clone().into());
                ui.set_status_message("Loaded recent logs.".into());
            }
            Err(err) => {
                self.recent_logs_text = format!("Failed to load logs: {err}");
                ui.set_log_view_text(self.recent_logs_text.clone().into());
                ui.set_status_message(format!("Log load failed: {err}").into());
            }
        }
    }

    fn load_editor_for_existing_job(&mut self, ui: &MainWindow, index: usize) {
        if index >= self.jobs.len() {
            return;
        }
        let job = self.jobs[index].clone();
        self.editor_target = Some(EditorTarget::Existing { index });
        match self.plist_service.load_document(&job.path) {
            Ok(document) => {
                self.editor_state = EditorState::from_document(&document, &self.plist_service);
                self.recent_logs_text = "Click 'Refresh Logs' to load runtime logs.".to_string();
                ui.set_log_view_text(self.recent_logs_text.clone().into());
                self.refresh_editor_preview(ui);
                self.sync_editor_to_ui(ui);
            }
            Err(err) => {
                self.editor_state = EditorState::default();
                self.sync_editor_to_ui(ui);
                self.recent_logs_text = "Log view unavailable: plist load failed.".to_string();
                ui.set_log_view_text(self.recent_logs_text.clone().into());
                ui.set_status_message(format!("Failed to load plist editor: {err}").into());
            }
        }
    }

    fn editor_document_from_state(&self) -> Result<StandardPlistDocument, String> {
        let start_interval = if self.editor_state.start_interval.trim().is_empty() {
            None
        } else {
            Some(
                self.editor_state
                    .start_interval
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| "StartInterval must be a non-negative integer".to_string())?,
            )
        };

        let environment_variables = self
            .plist_service
            .parse_env_pairs(&self.editor_state.environment_variables)
            .map_err(|err| err.to_string())?;

        Ok(StandardPlistDocument {
            label: self.editor_state.label.trim().to_string(),
            program: self.editor_state.program.trim().to_string(),
            program_arguments: self
                .plist_service
                .parse_program_arguments(&self.editor_state.program_arguments),
            run_at_load: self.editor_state.run_at_load,
            keep_alive: self.editor_state.keep_alive,
            start_interval,
            working_directory: (!self.editor_state.working_directory.trim().is_empty())
                .then_some(self.editor_state.working_directory.trim().to_string()),
            environment_variables,
        })
    }

    fn refresh_editor_preview(&mut self, ui: &MainWindow) {
        match self.editor_document_from_state() {
            Ok(document) => match self.plist_service.xml_preview(&document) {
                Ok(xml) => {
                    self.editor_state.xml_preview = xml;
                    let diagnostics = self.diagnostic_service.analyze(&document);
                    if diagnostics.is_empty() {
                        self.editor_state.diagnostics_text = "未发现问题。".to_string();
                    } else {
                        self.editor_state.diagnostics_text = diagnostics
                            .iter()
                            .map(|item| item.to_line())
                            .collect::<Vec<_>>()
                            .join("\n");
                    }
                }
                Err(err) => {
                    self.editor_state.xml_preview = format!("XML preview unavailable: {err}");
                    self.editor_state.diagnostics_text = format!("诊断暂不可用: {err}");
                }
            },
            Err(err) => {
                self.editor_state.xml_preview = format!("XML preview unavailable: {err}");
                self.editor_state.diagnostics_text = format!("诊断暂不可用: {err}");
            }
        }
        ui.set_editor_xml_preview(self.editor_state.xml_preview.clone().into());
        ui.set_editor_diagnostics_text(self.editor_state.diagnostics_text.clone().into());
    }

    fn sync_editor_to_ui(&self, ui: &MainWindow) {
        ui.set_editor_label(self.editor_state.label.clone().into());
        ui.set_editor_program(self.editor_state.program.clone().into());
        ui.set_editor_program_arguments(self.editor_state.program_arguments.clone().into());
        ui.set_editor_working_directory(self.editor_state.working_directory.clone().into());
        ui.set_editor_start_interval(self.editor_state.start_interval.clone().into());
        ui.set_editor_env_vars(self.editor_state.environment_variables.clone().into());
        ui.set_editor_run_at_load(self.editor_state.run_at_load);
        ui.set_editor_keep_alive(self.editor_state.keep_alive);
        ui.set_editor_xml_preview(self.editor_state.xml_preview.clone().into());
        ui.set_editor_diagnostics_text(self.editor_state.diagnostics_text.clone().into());
        ui.set_ai_prompt(self.editor_state.ai_prompt.clone().into());
        ui.set_ai_response(self.editor_state.ai_response.clone().into());
        let target_text = match &self.editor_target {
            Some(EditorTarget::Existing { index }) => self
                .jobs
                .get(*index)
                .map(|job| format!("Editing existing: {}", job.path.display()))
                .unwrap_or_else(|| "Editing existing job".to_string()),
            Some(EditorTarget::New { scope }) => {
                format!("Creating new {} plist", scope.as_str())
            }
            None => "No editor target selected".to_string(),
        };
        ui.set_editor_target_text(target_text.into());
    }

    fn apply_filters_and_render(&mut self, ui: &MainWindow, preferred_job_id: Option<&str>) {
        self.visible_indices = self
            .jobs
            .iter()
            .enumerate()
            .filter_map(|(idx, job)| {
                job_matches_filters(
                    job,
                    &self.search_query,
                    self.scope_filter,
                    &self.advanced_filter,
                    self.starred_only,
                )
                .then_some(idx)
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
            self.editor_target = None;
            self.editor_state = EditorState::default();
            self.sync_editor_to_ui(ui);
            self.recent_logs_text.clear();
            ui.set_log_view_text("".into());
            ui.set_selected_job_row(-1);
            self.update_selection_details(ui);
            return;
        }

        let preferred_index = preferred_job_id
            .and_then(|id| self.jobs.iter().position(|job| job.id == id))
            .filter(|idx| self.visible_indices.contains(idx));

        let selected = preferred_index
            .or(self
                .ui_state
                .selected_index
                .filter(|idx| self.visible_indices.contains(idx)))
            .unwrap_or(self.visible_indices[0]);
        self.ui_state.select(selected);
        self.ensure_runtime_details_loaded(ui, selected);
        self.load_editor_for_existing_job(ui, selected);
        self.update_selection_details(ui);
    }

    fn update_filter_badge(&self, ui: &MainWindow) {
        let scope_text = self
            .scope_filter
            .map_or("all scopes", ScopeFilter::label)
            .to_string();
        let mut tokens = vec![scope_text];
        if !self.search_query.is_empty() {
            tokens.push(format!("'{}'", self.search_query));
        }
        if self.starred_only {
            tokens.push("starred".to_string());
        }
        tokens.extend(self.advanced_filter.badge_tokens());
        ui.set_active_filter_text(tokens.join(" + ").into());
    }

    fn update_advanced_filter_controls(&self, ui: &MainWindow) {
        let status_text = self
            .advanced_filter
            .status
            .map_or("Status:any".to_string(), |status| {
                format!("Status:{}", status.as_str())
            });
        ui.set_status_filter_text(status_text.into());
        ui.set_disabled_filter_text(
            format!("Disabled:{}", self.advanced_filter.disabled.as_badge()).into(),
        );
        ui.set_run_at_load_filter_text(
            format!("RunAtLoad:{}", self.advanced_filter.run_at_load.as_badge()).into(),
        );
        ui.set_keep_alive_filter_text(
            format!("KeepAlive:{}", self.advanced_filter.keep_alive.as_badge()).into(),
        );
        ui.set_error_filter_text(
            format!("HasError:{}", self.advanced_filter.has_error.as_badge()).into(),
        );
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
        let selected_row = self
            .visible_indices
            .iter()
            .position(|visible_index| *visible_index == index)
            .map(|idx| idx as i32)
            .unwrap_or(-1);
        ui.set_selected_job_row(selected_row);
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
        ui.set_detail_run_at_load(
            job.metadata
                .run_at_load
                .map(bool_to_badge)
                .unwrap_or("N/A")
                .into(),
        );
        ui.set_detail_keep_alive(
            job.metadata
                .keep_alive
                .map(bool_to_badge)
                .unwrap_or("N/A")
                .into(),
        );
        ui.set_detail_disabled_key(
            job.metadata
                .disabled
                .map(bool_to_badge)
                .unwrap_or("N/A")
                .into(),
        );
        ui.set_is_starred(job.is_starred);
        ui.set_advanced_detail_visible(self.advanced_details_visible);
        let details = self
            .runtime_details
            .get(&job.id)
            .cloned()
            .unwrap_or_default();
        ui.set_detail_pid(details.pid.unwrap_or_else(|| "N/A".to_string()).into());
        ui.set_detail_last_exit(
            details
                .last_exit_status
                .unwrap_or_else(|| "N/A".to_string())
                .into(),
        );
        ui.set_detail_last_run(details.last_run.unwrap_or_else(|| "N/A".to_string()).into());
        ui.set_detail_runtime_hint(details.raw_hint.unwrap_or_default().into());
        ui.set_can_trigger(job.capabilities.can_trigger);
        ui.set_can_delete(job.capabilities.can_delete);
    }

    fn ensure_runtime_details_loaded(&mut self, ui: &MainWindow, index: usize) {
        if index >= self.jobs.len() {
            return;
        }
        let job = self.jobs[index].clone();
        if self.runtime_details.contains_key(&job.id) {
            return;
        }

        match self.job_service.fetch_job_details(&job) {
            Ok(details) => {
                self.runtime_details.insert(job.id.clone(), details);
            }
            Err(err) => {
                self.runtime_details.insert(
                    job.id.clone(),
                    JobRuntimeDetails {
                        raw_hint: Some(format!("Failed to load runtime details: {err}")),
                        ..JobRuntimeDetails::default()
                    },
                );
                ui.set_status_message(format!("Runtime details unavailable: {err}").into());
            }
        }
    }

    fn formatted_job_details(&self, index: usize) -> String {
        let job = &self.jobs[index];
        let details = self
            .runtime_details
            .get(&job.id)
            .cloned()
            .unwrap_or_default();
        let mut lines = vec![
            "LaunchPad Job Details".to_string(),
            "----------------------".to_string(),
            format!("Label: {}", job.label),
            format!("Scope: {}", job.scope),
            format!("Status: {}", job.status_text()),
            format!("Path: {}", job.path.to_string_lossy()),
            format!("Starred: {}", if job.is_starred { "yes" } else { "no" }),
            format!(
                "RunAtLoad: {}",
                job.metadata.run_at_load.map(bool_to_badge).unwrap_or("N/A")
            ),
            format!(
                "KeepAlive: {}",
                job.metadata.keep_alive.map(bool_to_badge).unwrap_or("N/A")
            ),
            format!(
                "Disabled key: {}",
                job.metadata.disabled.map(bool_to_badge).unwrap_or("N/A")
            ),
            format!("PID: {}", details.pid.unwrap_or_else(|| "N/A".to_string())),
            format!(
                "Last Exit Status: {}",
                details
                    .last_exit_status
                    .unwrap_or_else(|| "N/A".to_string())
            ),
            format!(
                "Last Run: {}",
                details.last_run.unwrap_or_else(|| "N/A".to_string())
            ),
        ];

        if let Some(error) = &job.error {
            lines.push(format!("Warning: {error}"));
        }
        if let Some(hint) = details.raw_hint {
            lines.push(format!("Runtime Hint: {hint}"));
        }

        lines.join("\n")
    }
}

fn job_matches_filters(
    job: &JobSummary,
    search_query: &str,
    scope_filter: Option<ScopeFilter>,
    advanced_filter: &AdvancedFilter,
    starred_only: bool,
) -> bool {
    let scope_matches = scope_filter.is_none_or(|filter| filter.matches(&job.scope));
    if starred_only && !job.is_starred {
        return false;
    }

    let normalized_query = search_query.trim().to_ascii_lowercase();
    if normalized_query.is_empty() {
        return scope_matches && advanced_filter.matches(job);
    }

    let label_matches = job.label.to_ascii_lowercase().contains(&normalized_query);
    let path_matches = job
        .path
        .to_string_lossy()
        .to_ascii_lowercase()
        .contains(&normalized_query);
    scope_matches && advanced_filter.matches(job) && (label_matches || path_matches)
}

fn clear_details(ui: &MainWindow) {
    ui.set_detail_label("".into());
    ui.set_detail_scope("".into());
    ui.set_detail_status("".into());
    ui.set_detail_path("".into());
    ui.set_detail_pid("".into());
    ui.set_detail_last_exit("".into());
    ui.set_detail_last_run("".into());
    ui.set_detail_runtime_hint("".into());
    ui.set_detail_error("".into());
    ui.set_detail_run_at_load("".into());
    ui.set_detail_keep_alive("".into());
    ui.set_detail_disabled_key("".into());
    ui.set_confirm_delete_visible(false);
    ui.set_confirm_delete_label("".into());
    ui.set_is_starred(false);
    ui.set_selected_job_row(-1);
    ui.set_advanced_detail_visible(false);
    ui.set_can_trigger(false);
    ui.set_can_delete(false);
}

fn bool_to_badge(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn sort_jobs_by_star_then_label(jobs: &mut [JobSummary]) {
    jobs.sort_by(|left, right| {
        right
            .is_starred
            .cmp(&left.is_starred)
            .then_with(|| left.label.cmp(&right.label))
    });
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
        ui.on_command_query_changed(move |query| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .command_query_changed(&ui, query.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_command_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().run_command_palette(&ui);
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
        ui.on_status_filter_requested(move |status| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .status_filter_requested(&ui, status.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_cycle_disabled_filter_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().cycle_disabled_filter(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_cycle_run_at_load_filter_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().cycle_run_at_load_filter(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_cycle_keep_alive_filter_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().cycle_keep_alive_filter(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_cycle_error_filter_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().cycle_error_filter(&ui);
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

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_toggle_star_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().toggle_star(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_copy_detail_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().copy_selected_details(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_toggle_advanced_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().toggle_advanced_details(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_editor_label_changed(move |value| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .editor_label_changed(&ui, value.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_editor_program_changed(move |value| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .editor_program_changed(&ui, value.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_editor_args_changed(move |value| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .editor_args_changed(&ui, value.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_editor_working_dir_changed(move |value| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .editor_working_dir_changed(&ui, value.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_editor_start_interval_changed(move |value| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .editor_start_interval_changed(&ui, value.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_editor_env_changed(move |value| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .editor_env_changed(&ui, value.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_ai_prompt_changed(move |value| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .ai_prompt_changed(&ui, value.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_ai_request_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().run_ai_assistant(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_toggle_editor_run_at_load(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().toggle_editor_run_at_load(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_toggle_editor_keep_alive(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().toggle_editor_keep_alive(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_save_editor_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().save_editor(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_refresh_logs_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().load_recent_logs(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_new_job_requested(move |scope| {
            if let Some(ui) = ui_weak.upgrade() {
                let parsed_scope = match scope.as_str() {
                    "global-agent" => JobScope::GlobalAgent,
                    "system-daemon" => JobScope::SystemDaemon,
                    _ => JobScope::UserAgent,
                };
                controller
                    .borrow_mut()
                    .start_new_job_editor(&ui, parsed_scope);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_star_filter_toggled(move |enabled| {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().set_starred_only(&ui, enabled);
            }
        });
    }

    controller.borrow().update_advanced_filter_controls(&ui);
    controller.borrow().sync_editor_to_ui(&ui);
    ui.set_command_query("".into());
    ui.set_log_view_text("Select a job to inspect logs.".into());
    controller.borrow_mut().refresh(&ui);
    ui.run()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use launchpad::domain::filter::{AdvancedFilter, TriStateFilter};
    use launchpad::domain::job::{JobCapabilities, JobScope, JobSummary};
    use launchpad::domain::status::JobStatus;

    use super::{job_matches_filters, ScopeFilter};

    #[test]
    fn filter_matches_by_scope() {
        let job = job_fixture(
            "com.demo.user",
            "/Users/test/Library/LaunchAgents/com.demo.user.plist",
        );
        assert!(job_matches_filters(
            &job,
            "",
            Some(ScopeFilter::UserAgent),
            &AdvancedFilter::default(),
            false
        ));
        assert!(!job_matches_filters(
            &job,
            "",
            Some(ScopeFilter::SystemDaemon),
            &AdvancedFilter::default(),
            false
        ));
    }

    #[test]
    fn filter_matches_by_label_and_path_query() {
        let job = job_fixture(
            "com.demo.searchable",
            "/Users/test/Library/LaunchAgents/com.demo.searchable.plist",
        );
        assert!(job_matches_filters(
            &job,
            "searchable",
            None,
            &AdvancedFilter::default(),
            false
        ));
        assert!(job_matches_filters(
            &job,
            "launchagents",
            None,
            &AdvancedFilter::default(),
            false
        ));
        assert!(!job_matches_filters(
            &job,
            "missing-token",
            None,
            &AdvancedFilter::default(),
            false
        ));
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
            Some(ScopeFilter::SystemDaemon),
            &AdvancedFilter::default(),
            false
        ));
        assert!(!job_matches_filters(
            &job,
            "demo.scope",
            Some(ScopeFilter::UserAgent),
            &AdvancedFilter::default(),
            false
        ));
    }

    #[test]
    fn filter_respects_starred_only_toggle() {
        let mut job = job_fixture(
            "com.demo.star",
            "/Users/test/Library/LaunchAgents/com.demo.star.plist",
        );
        job.is_starred = true;
        assert!(job_matches_filters(
            &job,
            "",
            None,
            &AdvancedFilter::default(),
            true
        ));

        job.is_starred = false;
        assert!(!job_matches_filters(
            &job,
            "",
            None,
            &AdvancedFilter::default(),
            true
        ));
    }

    #[test]
    fn filter_matches_advanced_flags() {
        let mut job = job_fixture(
            "com.demo.meta",
            "/Users/test/Library/LaunchAgents/com.demo.meta.plist",
        );
        job.metadata.run_at_load = Some(true);
        job.metadata.keep_alive = Some(false);
        job.metadata.disabled = Some(false);

        let filter = AdvancedFilter {
            run_at_load: TriStateFilter::Yes,
            keep_alive: TriStateFilter::No,
            disabled: TriStateFilter::No,
            ..AdvancedFilter::default()
        };
        assert!(job_matches_filters(&job, "", None, &filter, false));

        let strict_filter = AdvancedFilter {
            disabled: TriStateFilter::Yes,
            ..filter
        };
        assert!(!job_matches_filters(&job, "", None, &strict_filter, false));
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
            is_starred: false,
            metadata: launchpad::domain::job::JobMetadata::default(),
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
