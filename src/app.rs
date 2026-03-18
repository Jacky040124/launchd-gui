use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use launchpad::adapter::clipboard::{ClipboardClient, SystemClipboardClient};
use launchpad::adapter::fs_ops::SystemFsOps;
use launchpad::adapter::fs_scan::FileScanner;
use launchpad::adapter::launchctl::{current_uid, SystemLaunchctlClient};
use launchpad::adapter::log_stream::SystemLogStreamClient;
use launchpad::adapter::plist_doc::SystemPlistDocumentStore;
use launchpad::adapter::plist_reader::SystemPlistReader;
use launchpad::adapter::quicklaunch::{
    BridgeQuickLaunchProvider, FileQuickLaunchProvider, NoopQuickLaunchProvider, QuickLaunchAction,
    QuickLaunchProvider,
};
use launchpad::adapter::star_store::JsonStarStore;
use launchpad::config::AppConfig;
use launchpad::domain::action::TriggerAction;
use launchpad::domain::filter::AdvancedFilter;
use launchpad::domain::job::{compute_capabilities, JobScope, JobSummary};
use launchpad::domain::job_detail::JobRuntimeDetails;
use launchpad::domain::plist_document::{search_key_defs, LaunchdKeyDef, StandardPlistDocument};
use launchpad::domain::status::JobStatus;
use launchpad::service::action_service::ActionService;
use launchpad::service::ai_service::AiService;
use launchpad::service::delete_service::DeleteService;
use launchpad::service::diagnostic_service::DiagnosticService;
use launchpad::service::job_service::JobService;
use launchpad::service::log_service::LogService;
use launchpad::service::plist_service::PlistService;
use launchpad::service::quicklaunch_service::{
    QuickLaunchConfig, QuickLaunchGroupBy, QuickLaunchService,
};
use launchpad::service::star_service::StarService;
use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};
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
    expert_entries: String,
    xml_preview: String,
    diagnostics_text: String,
    ai_prompt: String,
    ai_response: String,
    ai_diff_preview: String,
    log_live_mode: bool,
    log_window_minutes: String,
    log_stream_seconds: String,
    log_max_lines: String,
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
            expert_entries: plist_service.format_extra_pairs(&document.extra_string_keys),
            xml_preview: String::new(),
            diagnostics_text: String::new(),
            ai_prompt: String::new(),
            ai_response: String::new(),
            ai_diff_preview: String::new(),
            log_live_mode: false,
            log_window_minutes: "10".to_string(),
            log_stream_seconds: "5".to_string(),
            log_max_lines: "120".to_string(),
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
    quicklaunch_service: QuickLaunchService,
    quicklaunch_action_poll_ms: u64,
    plist_service: PlistService,
    star_service: StarService,
    clipboard: Arc<dyn ClipboardClient>,
    jobs: Vec<JobSummary>,
    visible_indices: Vec<usize>,
    runtime_details: HashMap<String, JobRuntimeDetails>,
    search_query: String,
    command_query: String,
    key_panel_query: String,
    visible_key_defs: Vec<&'static LaunchdKeyDef>,
    scope_filter: Option<ScopeFilter>,
    advanced_filter: AdvancedFilter,
    starred_only: bool,
    advanced_details_visible: bool,
    recent_logs_text: String,
    editor_state: EditorState,
    editor_target: Option<EditorTarget>,
    pending_ai_patch_document: Option<StandardPlistDocument>,
    pending_ai_actions: Vec<TriggerAction>,
    ai_patch_confirmation_required: bool,
    ui_state: UiState,
}

impl AppController {
    fn new() -> Self {
        let config = AppConfig::from_env();
        let launchctl = Arc::new(SystemLaunchctlClient);
        let plist_reader = Arc::new(SystemPlistReader);
        let fs_ops = Arc::new(SystemFsOps);
        let clipboard = Arc::new(SystemClipboardClient);
        let uid = current_uid();
        let quicklaunch_provider: Arc<dyn QuickLaunchProvider> = if config.quicklaunch_enabled {
            BridgeQuickLaunchProvider::from_env()
                .map(|provider| Arc::new(provider) as Arc<dyn QuickLaunchProvider>)
                .unwrap_or_else(|| {
                    Arc::new(FileQuickLaunchProvider::new_default()) as Arc<dyn QuickLaunchProvider>
                })
        } else {
            Arc::new(NoopQuickLaunchProvider) as Arc<dyn QuickLaunchProvider>
        };
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
            ai_service: AiService::new_with_default(&config.ai_default_provider),
            quicklaunch_service: QuickLaunchService::new(
                quicklaunch_provider,
                QuickLaunchConfig {
                    enabled: config.quicklaunch_enabled,
                    starred_only: config.quicklaunch_starred_only,
                    max_items: config.quicklaunch_max_items,
                    group_by: QuickLaunchGroupBy::from_env_value(&config.quicklaunch_group_by),
                },
            ),
            quicklaunch_action_poll_ms: config.quicklaunch_action_poll_ms,
            plist_service: PlistService::new(Arc::new(SystemPlistDocumentStore)),
            star_service,
            clipboard,
            jobs: Vec::new(),
            visible_indices: Vec::new(),
            runtime_details: HashMap::new(),
            search_query: String::new(),
            command_query: String::new(),
            key_panel_query: String::new(),
            visible_key_defs: search_key_defs(""),
            scope_filter: None,
            advanced_filter: AdvancedFilter::default(),
            starred_only: false,
            advanced_details_visible: false,
            recent_logs_text: String::new(),
            editor_state: EditorState::default(),
            editor_target: None,
            pending_ai_patch_document: None,
            pending_ai_actions: Vec::new(),
            ai_patch_confirmation_required: false,
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
                    self.pending_ai_patch_document = None;
                    self.pending_ai_actions.clear();
                    self.ai_patch_confirmation_required = false;
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
                let quicklaunch_note = match self.quicklaunch_service.sync_jobs(&self.jobs) {
                    Ok(0) => String::new(),
                    Ok(count) => format!(" QuickLaunch synced {count} items."),
                    Err(err) => format!(" QuickLaunch sync skipped: {err}."),
                };
                let quicklaunch_action_note = self.process_quicklaunch_actions();
                let visible_count = self.visible_indices.len();
                if visible_count == 0 {
                    ui.set_status_message(
                        format!(
                            "Loaded {} jobs. No jobs match current filters.{}{}",
                            self.jobs.len(),
                            quicklaunch_note,
                            quicklaunch_action_note
                        )
                        .into(),
                    );
                } else {
                    ui.set_status_message(
                        format!(
                            "Loaded {} jobs ({} visible).{}{}",
                            self.jobs.len(),
                            visible_count,
                            quicklaunch_note,
                            quicklaunch_action_note
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
                self.pending_ai_patch_document = None;
                self.pending_ai_actions.clear();
                self.ai_patch_confirmation_required = false;
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
        self.pending_ai_patch_document = None;
        self.pending_ai_actions.clear();
        self.ai_patch_confirmation_required = false;
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

        if let Some(provider_name) = normalized.strip_prefix("provider ") {
            let provider_name = provider_name.trim();
            if provider_name.is_empty() {
                ui.set_status_message("Usage: provider <name>".into());
                return;
            }
            if self.ai_service.set_active_provider(provider_name) {
                self.sync_editor_to_ui(ui);
                ui.set_status_message(format!("Switched AI provider to {provider_name}.").into());
            } else {
                let available = self.ai_service.available_providers().join(", ");
                ui.set_status_message(
                    format!(
                        "Unknown provider '{}'. Available: {}",
                        provider_name, available
                    )
                    .into(),
                );
            }
            return;
        }

        if normalized == "providers" {
            let available = self.ai_service.available_providers().join(", ");
            ui.set_status_message(format!("Available AI providers: {}", available).into());
            return;
        }

        match normalized.as_str() {
            "refresh" => self.refresh(ui),
            "start" => self.trigger(ui, TriggerAction::Start),
            "stop" => self.trigger(ui, TriggerAction::Stop),
            "kickstart" | "restart" => self.trigger(ui, TriggerAction::Kickstart),
            "enable" => self.trigger(ui, TriggerAction::Enable),
            "disable" => self.trigger(ui, TriggerAction::Disable),
            "load" => self.trigger(ui, TriggerAction::Load),
            "unload" => self.trigger(ui, TriggerAction::Unload),
            "start starred" => self.trigger_starred_batch(ui, TriggerAction::Start),
            "stop starred" => self.trigger_starred_batch(ui, TriggerAction::Stop),
            "enable starred" => self.trigger_starred_batch(ui, TriggerAction::Enable),
            "disable starred" => self.trigger_starred_batch(ui, TriggerAction::Disable),
            "load starred" => self.trigger_starred_batch(ui, TriggerAction::Load),
            "unload starred" => self.trigger_starred_batch(ui, TriggerAction::Unload),
            "restart starred" | "kickstart starred" => {
                self.trigger_starred_batch(ui, TriggerAction::Kickstart)
            }
            "save" => self.save_editor(ui),
            "save load" => self.save_editor_and_load(ui),
            "save load enable" => self.save_editor_load_enable(ui),
            "ai actions" | "run ai actions" => self.run_ai_suggested_actions(ui),
            "logs" => self.load_recent_logs(ui),
            "logs live" => {
                self.editor_state.log_live_mode = true;
                ui.set_log_live_mode(true);
                ui.set_log_mode_text("Mode:Live".into());
                self.load_recent_logs(ui);
            }
            "logs history" => {
                self.editor_state.log_live_mode = false;
                ui.set_log_live_mode(false);
                ui.set_log_mode_text("Mode:History".into());
                self.load_recent_logs(ui);
            }
            "star" | "unstar" => self.toggle_star(ui),
            "new user" | "new user job" => self.start_new_job_editor(ui, JobScope::UserAgent),
            "new global" | "new global job" => self.start_new_job_editor(ui, JobScope::GlobalAgent),
            _ => {
                ui.set_status_message(
                    format!(
                        "Unknown command '{}'. Try: refresh/start/stop/restart/enable/load/new user/save/ai actions/logs/logs live/start starred",
                        normalized
                    )
                    .into(),
                );
            }
        }
    }

    fn trigger_starred_batch(&mut self, ui: &MainWindow, action: TriggerAction) {
        let targets = self
            .jobs
            .iter()
            .filter(|job| job.is_starred && job.capabilities.can_trigger)
            .cloned()
            .collect::<Vec<_>>();
        if targets.is_empty() {
            ui.set_status_message("No triggerable starred jobs found.".into());
            return;
        }

        let mut success = 0usize;
        let mut failed = 0usize;
        let mut first_error = None;
        for job in targets {
            match self.action_service.execute(&job, action) {
                Ok(_) => success += 1,
                Err(err) => {
                    failed += 1;
                    if first_error.is_none() {
                        first_error = Some(format!("{}: {err}", job.label));
                    }
                }
            }
        }

        self.refresh(ui);
        if let Some(error) = first_error {
            ui.set_status_message(
                format!(
                    "Batch {} finished: {} success, {} failed. First error: {}",
                    action.as_str(),
                    success,
                    failed,
                    error
                )
                .into(),
            );
        } else {
            ui.set_status_message(
                format!(
                    "Batch {} finished: {} success, {} failed.",
                    action.as_str(),
                    success,
                    failed
                )
                .into(),
            );
        }
    }

    fn process_quicklaunch_actions(&mut self) -> String {
        let actions = match self.quicklaunch_service.drain_actions() {
            Ok(actions) => actions,
            Err(err) => return format!(" QuickLaunch actions skipped: {err}."),
        };
        if actions.is_empty() {
            return String::new();
        }

        let mut success = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;

        for action in actions {
            let Some(trigger) = TriggerAction::from_ui_value(action.action.as_str()) else {
                skipped += action.job_ids.len().max(1);
                continue;
            };
            let (action_success, action_failed, action_skipped) =
                self.execute_quicklaunch_action(&action, trigger);
            success += action_success;
            failed += action_failed;
            skipped += action_skipped;
        }

        format!(
            " QuickLaunch actions: {} success, {} failed, {} skipped.",
            success, failed, skipped
        )
    }

    fn poll_quicklaunch_actions(&mut self, ui: &MainWindow) {
        if self.jobs.is_empty() {
            return;
        }
        let note = self.process_quicklaunch_actions();
        if note.trim().is_empty() {
            return;
        }
        self.refresh(ui);
        ui.set_status_message(format!("Auto-applied queued QuickLaunch actions.{note}").into());
    }

    fn execute_quicklaunch_action(
        &self,
        queued_action: &QuickLaunchAction,
        trigger: TriggerAction,
    ) -> (usize, usize, usize) {
        let mut success = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;
        for job_id in &queued_action.job_ids {
            let Some(job) = self.jobs.iter().find(|job| &job.id == job_id) else {
                skipped += 1;
                continue;
            };
            match self.action_service.execute(job, trigger) {
                Ok(_) => success += 1,
                Err(_) => failed += 1,
            }
        }

        (success, failed, skipped)
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

    fn editor_expert_entries_changed(&mut self, ui: &MainWindow, value: &str) {
        self.editor_state.expert_entries = value.trim().to_string();
        self.refresh_editor_preview(ui);
    }

    fn log_window_minutes_changed(&mut self, ui: &MainWindow, value: &str) {
        self.editor_state.log_window_minutes = value.trim().to_string();
        ui.set_log_window_minutes(self.editor_state.log_window_minutes.clone().into());
    }

    fn log_stream_seconds_changed(&mut self, ui: &MainWindow, value: &str) {
        self.editor_state.log_stream_seconds = value.trim().to_string();
        ui.set_log_stream_seconds(self.editor_state.log_stream_seconds.clone().into());
    }

    fn log_max_lines_changed(&mut self, ui: &MainWindow, value: &str) {
        self.editor_state.log_max_lines = value.trim().to_string();
        ui.set_log_max_lines(self.editor_state.log_max_lines.clone().into());
    }

    fn toggle_log_live_mode(&mut self, ui: &MainWindow) {
        self.editor_state.log_live_mode = !self.editor_state.log_live_mode;
        ui.set_log_live_mode(self.editor_state.log_live_mode);
        ui.set_log_mode_text(
            if self.editor_state.log_live_mode {
                "Mode:Live"
            } else {
                "Mode:History"
            }
            .into(),
        );
    }

    fn key_panel_query_changed(&mut self, ui: &MainWindow, query: &str) {
        self.key_panel_query = query.to_string();
        self.visible_key_defs = search_key_defs(query);
        self.sync_key_panel_to_ui(ui);
    }

    fn add_key_from_panel(&mut self, ui: &MainWindow, index: usize) {
        let Some(def) = self.visible_key_defs.get(index).copied() else {
            return;
        };

        let mut expert_map = self
            .plist_service
            .parse_extra_pairs(&self.editor_state.expert_entries)
            .unwrap_or_default();

        match def.key {
            "Label" => {
                if self.editor_state.label.trim().is_empty() {
                    self.editor_state.label = "com.example.new-job".to_string();
                }
            }
            "Program" => {
                if self.editor_state.program.trim().is_empty() {
                    self.editor_state.program = "/usr/bin/true".to_string();
                }
            }
            "ProgramArguments" => {
                if self.editor_state.program_arguments.trim().is_empty() {
                    self.editor_state.program_arguments = if self.editor_state.program.is_empty() {
                        "/usr/bin/true".to_string()
                    } else {
                        self.editor_state.program.clone()
                    };
                }
            }
            "RunAtLoad" => {
                self.editor_state.run_at_load = true;
            }
            "KeepAlive" => {
                self.editor_state.keep_alive = true;
            }
            "StartInterval" => {
                if self.editor_state.start_interval.trim().is_empty() {
                    self.editor_state.start_interval = "60".to_string();
                }
            }
            "WorkingDirectory" => {
                if self.editor_state.working_directory.trim().is_empty() {
                    self.editor_state.working_directory = "/tmp".to_string();
                }
            }
            "EnvironmentVariables" => {
                if self.editor_state.environment_variables.trim().is_empty() {
                    self.editor_state.environment_variables =
                        "PATH=/usr/bin:/bin:/usr/sbin:/sbin".to_string();
                }
            }
            other => {
                expert_map
                    .entry(other.to_string())
                    .or_insert_with(|| "<value>".to_string());
                self.editor_state.expert_entries =
                    self.plist_service.format_extra_pairs(&expert_map);
            }
        }

        self.sync_editor_to_ui(ui);
        self.refresh_editor_preview(ui);
        ui.set_status_message(format!("Added key template '{}'.", def.key).into());
    }

    fn ai_prompt_changed(&mut self, ui: &MainWindow, value: &str) {
        self.editor_state.ai_prompt = value.to_string();
        self.pending_ai_actions.clear();
        ui.set_ai_actions_ready(false);
        ui.set_ai_prompt(self.editor_state.ai_prompt.clone().into());
    }

    fn cycle_ai_provider(&mut self, ui: &MainWindow) {
        let selected = self.ai_service.cycle_provider();
        ui.set_ai_provider_text(format!("AI Provider: {selected}").into());
        ui.set_status_message(format!("Switched AI provider to {selected}.").into());
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
            .suggest_edit_with_stream(&self.editor_state.ai_prompt, &self.editor_state.xml_preview)
        {
            Ok(stream) => {
                let response = stream.response;
                let provider = response.provider;
                let summary = response.summary;
                let suggested_patch_notes = response.suggested_patch_notes;
                let suggested_actions_raw = response.suggested_actions;
                let mut lines = vec![format!("Provider: {}\nSummary: {}", provider, summary)];
                if !stream.chunks.is_empty() {
                    lines.push("Stream preview:".to_string());
                    lines.extend(stream.chunks.into_iter().map(|chunk| format!("> {chunk}")));
                }
                if !suggested_patch_notes.is_empty() {
                    lines.push("Suggestions:".to_string());
                    lines.extend(
                        suggested_patch_notes
                            .into_iter()
                            .map(|item| format!("- {item}")),
                    );
                }
                self.pending_ai_actions = suggested_actions_raw
                    .iter()
                    .filter_map(|action| TriggerAction::from_ui_value(action))
                    .collect();
                if !suggested_actions_raw.is_empty() {
                    lines.push("Suggested Actions:".to_string());
                    lines.extend(
                        suggested_actions_raw
                            .into_iter()
                            .map(|item| format!("* {item}")),
                    );
                }
                self.editor_state.ai_response = lines.join("\n");
                ui.set_ai_response(self.editor_state.ai_response.clone().into());
                ui.set_ai_actions_ready(!self.pending_ai_actions.is_empty());
                self.preview_ai_patch(ui);
                ui.set_status_message("AI suggestions updated.".into());
            }
            Err(err) => {
                self.editor_state.ai_response = format!("AI request failed: {err}");
                ui.set_ai_response(self.editor_state.ai_response.clone().into());
                self.pending_ai_patch_document = None;
                self.pending_ai_actions.clear();
                self.ai_patch_confirmation_required = false;
                self.editor_state.ai_diff_preview.clear();
                ui.set_ai_diff_preview("".into());
                ui.set_ai_patch_ready(false);
                ui.set_ai_actions_ready(false);
                ui.set_status_message(format!("AI request failed: {err}").into());
            }
        }
    }

    fn request_apply_ai_patch(&mut self, ui: &MainWindow) {
        if self.pending_ai_patch_document.is_none() {
            ui.set_status_message("No AI patch preview available.".into());
            return;
        }
        self.ai_patch_confirmation_required = true;
        ui.set_ai_patch_ready(true);
        ui.set_status_message("AI patch ready. Click confirm to apply to editor.".into());
    }

    fn confirm_apply_ai_patch(&mut self, ui: &MainWindow) {
        if !self.ai_patch_confirmation_required {
            ui.set_status_message("No pending AI patch confirmation.".into());
            return;
        }
        let Some(document) = self.pending_ai_patch_document.clone() else {
            ui.set_status_message("No AI patch data available.".into());
            return;
        };

        self.editor_state = EditorState::from_document(&document, &self.plist_service);
        self.pending_ai_patch_document = None;
        self.ai_patch_confirmation_required = false;
        self.editor_state.ai_diff_preview.clear();
        self.refresh_editor_preview(ui);
        self.sync_editor_to_ui(ui);
        ui.set_ai_patch_ready(false);
        ui.set_ai_diff_preview("".into());
        ui.set_status_message(
            "AI patch applied to editor. Review and click Save when ready.".into(),
        );
    }

    fn run_ai_suggested_actions(&mut self, ui: &MainWindow) {
        if self.pending_ai_actions.is_empty() {
            ui.set_status_message("No AI suggested actions available.".into());
            return;
        }
        let Some(index) = self.ui_state.selected_index else {
            ui.set_status_message("Select a job first to run AI actions.".into());
            return;
        };
        let Some(job) = self.jobs.get(index).cloned() else {
            ui.set_status_message("Selected job is no longer available.".into());
            return;
        };

        let actions = self.pending_ai_actions.clone();
        let mut success = 0usize;
        let mut failed = 0usize;
        for action in actions {
            match self.action_service.execute(&job, action) {
                Ok(_) => success += 1,
                Err(_) => failed += 1,
            }
        }

        self.pending_ai_actions.clear();
        ui.set_ai_actions_ready(false);
        self.refresh(ui);
        ui.set_status_message(
            format!(
                "AI actions finished: {} success, {} failed.",
                success, failed
            )
            .into(),
        );
    }

    fn preview_ai_patch(&mut self, ui: &MainWindow) {
        let current_document = match self.editor_document_from_state() {
            Ok(document) => document,
            Err(_) => {
                self.pending_ai_patch_document = None;
                self.ai_patch_confirmation_required = false;
                self.editor_state.ai_diff_preview =
                    "Cannot build AI patch preview: current editor state is invalid.".to_string();
                ui.set_ai_diff_preview(self.editor_state.ai_diff_preview.clone().into());
                ui.set_ai_patch_ready(false);
                return;
            }
        };

        let Some(candidate) = self.ai_patch_candidate_from_prompt(&current_document) else {
            self.pending_ai_patch_document = None;
            self.ai_patch_confirmation_required = false;
            self.editor_state.ai_diff_preview =
                "No deterministic patch could be inferred from prompt.".to_string();
            ui.set_ai_diff_preview(self.editor_state.ai_diff_preview.clone().into());
            ui.set_ai_patch_ready(false);
            return;
        };

        if candidate == current_document {
            self.pending_ai_patch_document = None;
            self.ai_patch_confirmation_required = false;
            self.editor_state.ai_diff_preview =
                "AI patch preview detected no effective changes.".to_string();
            ui.set_ai_diff_preview(self.editor_state.ai_diff_preview.clone().into());
            ui.set_ai_patch_ready(false);
            return;
        }

        let before_xml = self
            .plist_service
            .xml_preview(&current_document)
            .unwrap_or_default();
        let after_xml = self
            .plist_service
            .xml_preview(&candidate)
            .unwrap_or_default();
        self.editor_state.ai_diff_preview = build_line_diff(&before_xml, &after_xml);
        self.pending_ai_patch_document = Some(candidate);
        self.ai_patch_confirmation_required = false;
        ui.set_ai_diff_preview(self.editor_state.ai_diff_preview.clone().into());
        ui.set_ai_patch_ready(true);
    }

    fn ai_patch_candidate_from_prompt(
        &self,
        current_document: &StandardPlistDocument,
    ) -> Option<StandardPlistDocument> {
        let normalized = self.editor_state.ai_prompt.to_ascii_lowercase();
        let mut candidate = current_document.clone();
        let mut changed = false;

        if (normalized.contains("run at load") || normalized.contains("开机"))
            && !candidate.run_at_load
        {
            candidate.run_at_load = true;
            changed = true;
        }

        if (normalized.contains("keep alive") || normalized.contains("常驻"))
            && !candidate.keep_alive
        {
            candidate.keep_alive = true;
            changed = true;
        }

        if normalized.contains("disable keep alive")
            || normalized.contains("keepalive off")
            || normalized.contains("关闭常驻") && candidate.keep_alive
        {
            candidate.keep_alive = false;
            changed = true;
        }

        if normalized.contains("interval") || normalized.contains("定时") {
            if let Some(interval) = first_u64_in_text(&normalized) {
                if candidate.start_interval != Some(interval) {
                    candidate.start_interval = Some(interval);
                    changed = true;
                }
            }
        }

        if normalized.contains("log") || normalized.contains("日志") {
            if !candidate.extra_string_keys.contains_key("StandardOutPath") {
                candidate.extra_string_keys.insert(
                    "StandardOutPath".to_string(),
                    "/tmp/launchpad.out.log".to_string(),
                );
                changed = true;
            }
            if !candidate
                .extra_string_keys
                .contains_key("StandardErrorPath")
            {
                candidate.extra_string_keys.insert(
                    "StandardErrorPath".to_string(),
                    "/tmp/launchpad.err.log".to_string(),
                );
                changed = true;
            }
        }

        changed.then_some(candidate)
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
        self.save_editor_with_post_actions(ui, false, false);
    }

    fn save_editor_and_load(&mut self, ui: &MainWindow) {
        self.save_editor_with_post_actions(ui, true, false);
    }

    fn save_editor_load_enable(&mut self, ui: &MainWindow) {
        self.save_editor_with_post_actions(ui, true, true);
    }

    fn save_editor_with_post_actions(
        &mut self,
        ui: &MainWindow,
        should_load: bool,
        should_enable: bool,
    ) {
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

        let mut post_action_job: Option<JobSummary> = None;
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
                        post_action_job = Some(JobSummary {
                            id: job.id.clone(),
                            label: document.label.clone(),
                            path: job.path.clone(),
                            scope: job.scope.clone(),
                            status: job.status,
                            is_starred: job.is_starred,
                            metadata: job.metadata.clone(),
                            capabilities: compute_capabilities(&job.scope, &job.path),
                            error: None,
                        });
                    }
                    Err(err) => ui.set_status_message(format!("Save failed: {err}").into()),
                }
            }
            EditorTarget::New { scope } => {
                let create_scope = scope.clone();
                match self.plist_service.create_new(create_scope, &document) {
                    Ok(path) => {
                        post_action_job = Some(JobSummary {
                            id: path.to_string_lossy().to_string(),
                            label: document.label.clone(),
                            path: path.clone(),
                            scope: scope.clone(),
                            status: JobStatus::Unknown,
                            is_starred: false,
                            metadata: launchpad::domain::job::JobMetadata::default(),
                            capabilities: compute_capabilities(&scope, &path),
                            error: None,
                        });
                    }
                    Err(err) => ui.set_status_message(format!("Create failed: {err}").into()),
                }
            }
        }

        let mut load_result = None;
        let mut enable_result = None;
        if let Some(job) = post_action_job.as_ref() {
            if should_load {
                load_result = Some(self.action_service.execute(job, TriggerAction::Load));
            }
            if should_enable {
                enable_result = Some(self.action_service.execute(job, TriggerAction::Enable));
            }
        } else {
            return;
        }

        self.refresh(ui);
        let mut status_line = "Saved plist changes.".to_string();
        if should_load {
            status_line = match load_result {
                Some(Ok(())) => format!("{status_line} Load: success."),
                Some(Err(err)) => format!("{status_line} Load failed: {err}."),
                None => format!("{status_line} Load skipped."),
            };
        }
        if should_enable {
            status_line = match enable_result {
                Some(Ok(())) => format!("{status_line} Enable: success."),
                Some(Err(err)) => format!("{status_line} Enable failed: {err}."),
                None => format!("{status_line} Enable skipped."),
            };
        }
        ui.set_status_message(status_line.into());
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
        let minutes = self
            .editor_state
            .log_window_minutes
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(10);
        let max_lines = self
            .editor_state
            .log_max_lines
            .trim()
            .parse::<usize>()
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(120);
        let stream_seconds = self
            .editor_state
            .log_stream_seconds
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(5);
        let log_result = if self.editor_state.log_live_mode {
            self.log_service
                .live_logs(&label, stream_seconds, max_lines)
        } else {
            self.log_service.recent_logs(&label, minutes, max_lines)
        };

        match log_result {
            Ok(logs) => {
                self.recent_logs_text = if logs.trim().is_empty() {
                    if self.editor_state.log_live_mode {
                        "No live logs captured.".to_string()
                    } else {
                        "No recent logs found.".to_string()
                    }
                } else {
                    logs
                };
                ui.set_log_view_text(self.recent_logs_text.clone().into());
                let mode_text = if self.editor_state.log_live_mode {
                    format!("live={}s", stream_seconds)
                } else {
                    format!("history={}m", minutes)
                };
                ui.set_status_message(
                    format!("Loaded logs ({mode_text}, max_lines={max_lines}).").into(),
                );
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
                self.pending_ai_patch_document = None;
                self.pending_ai_actions.clear();
                self.ai_patch_confirmation_required = false;
                self.recent_logs_text = "Click 'Refresh Logs' to load runtime logs.".to_string();
                ui.set_log_view_text(self.recent_logs_text.clone().into());
                self.refresh_editor_preview(ui);
                self.sync_editor_to_ui(ui);
            }
            Err(err) => {
                self.editor_state = EditorState::default();
                self.pending_ai_patch_document = None;
                self.pending_ai_actions.clear();
                self.ai_patch_confirmation_required = false;
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
        let extra_string_keys = self
            .plist_service
            .parse_extra_pairs(&self.editor_state.expert_entries)
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
            extra_string_keys,
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
        ui.set_editor_expert_entries(self.editor_state.expert_entries.clone().into());
        ui.set_editor_run_at_load(self.editor_state.run_at_load);
        ui.set_editor_keep_alive(self.editor_state.keep_alive);
        ui.set_editor_xml_preview(self.editor_state.xml_preview.clone().into());
        ui.set_editor_diagnostics_text(self.editor_state.diagnostics_text.clone().into());
        ui.set_ai_prompt(self.editor_state.ai_prompt.clone().into());
        ui.set_ai_response(self.editor_state.ai_response.clone().into());
        ui.set_ai_diff_preview(self.editor_state.ai_diff_preview.clone().into());
        ui.set_log_window_minutes(self.editor_state.log_window_minutes.clone().into());
        ui.set_log_stream_seconds(self.editor_state.log_stream_seconds.clone().into());
        ui.set_log_max_lines(self.editor_state.log_max_lines.clone().into());
        ui.set_log_live_mode(self.editor_state.log_live_mode);
        ui.set_log_mode_text(
            if self.editor_state.log_live_mode {
                "Mode:Live"
            } else {
                "Mode:History"
            }
            .into(),
        );
        ui.set_ai_patch_ready(self.pending_ai_patch_document.is_some());
        ui.set_ai_actions_ready(!self.pending_ai_actions.is_empty());
        ui.set_ai_provider_text(
            format!("AI Provider: {}", self.ai_service.active_provider_name()).into(),
        );
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

    fn sync_key_panel_to_ui(&self, ui: &MainWindow) {
        let lines: Vec<SharedString> = self
            .visible_key_defs
            .iter()
            .map(|def| format!("{} — {}", def.key, def.note).into())
            .collect();
        ui.set_key_panel_lines(ModelRc::new(VecModel::from(lines)));
        ui.set_key_panel_query(self.key_panel_query.clone().into());
        ui.set_key_panel_count_text(format!("{} keys", self.visible_key_defs.len()).into());
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
            self.pending_ai_patch_document = None;
            self.pending_ai_actions.clear();
            self.ai_patch_confirmation_required = false;
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

fn first_u64_in_text(text: &str) -> Option<u64> {
    text.split_whitespace().find_map(|token| {
        token
            .trim_matches(|ch: char| !ch.is_ascii_digit())
            .parse::<u64>()
            .ok()
    })
}

fn build_line_diff(before: &str, after: &str) -> String {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let max_len = before_lines.len().max(after_lines.len());
    let mut diff_lines = Vec::new();

    for idx in 0..max_len {
        let left = before_lines.get(idx).copied().unwrap_or("");
        let right = after_lines.get(idx).copied().unwrap_or("");
        if left != right {
            if !left.is_empty() {
                diff_lines.push(format!("- {left}"));
            }
            if !right.is_empty() {
                diff_lines.push(format!("+ {right}"));
            }
        }
    }

    if diff_lines.is_empty() {
        "No line changes.".to_string()
    } else {
        diff_lines.join("\n")
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
        ui.on_editor_expert_entries_changed(move |value| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .editor_expert_entries_changed(&ui, value.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_log_window_minutes_changed(move |value| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .log_window_minutes_changed(&ui, value.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_log_stream_seconds_changed(move |value| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .log_stream_seconds_changed(&ui, value.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_log_max_lines_changed(move |value| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .log_max_lines_changed(&ui, value.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_toggle_log_mode_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().toggle_log_live_mode(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_key_panel_query_changed(move |query| {
            if let Some(ui) = ui_weak.upgrade() {
                controller
                    .borrow_mut()
                    .key_panel_query_changed(&ui, query.as_str());
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_key_panel_add_requested(move |index| {
            if let Some(ui) = ui_weak.upgrade() {
                if index >= 0 {
                    controller
                        .borrow_mut()
                        .add_key_from_panel(&ui, index as usize);
                }
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
        ui.on_ai_cycle_provider_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().cycle_ai_provider(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_ai_apply_patch_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().request_apply_ai_patch(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_ai_confirm_patch_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().confirm_apply_ai_patch(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_ai_run_actions_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().run_ai_suggested_actions(&ui);
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
        ui.on_save_editor_and_load_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().save_editor_and_load(&ui);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let controller = controller.clone();
        ui.on_save_editor_load_enable_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                controller.borrow_mut().save_editor_load_enable(&ui);
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
    controller.borrow().sync_key_panel_to_ui(&ui);
    ui.set_command_query("".into());
    ui.set_log_view_text("Select a job to inspect logs.".into());
    controller.borrow_mut().refresh(&ui);
    let quicklaunch_action_timer = Timer::default();
    let poll_ms = controller.borrow().quicklaunch_action_poll_ms;
    if poll_ms > 0 {
        let ui_weak = ui.as_weak();
        let controller_for_timer = controller.clone();
        quicklaunch_action_timer.start(
            TimerMode::Repeated,
            Duration::from_millis(poll_ms),
            move || {
                if let Some(ui) = ui_weak.upgrade() {
                    if ui.get_busy() {
                        return;
                    }
                    controller_for_timer
                        .borrow_mut()
                        .poll_quicklaunch_actions(&ui);
                }
            },
        );
    }
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
