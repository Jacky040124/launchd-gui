use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use launchpad::adapter::fs_ops::SystemFsOps;
use launchpad::adapter::fs_scan::FileScanner;
use launchpad::adapter::launchctl::{current_uid, SystemLaunchctlClient};
use launchpad::adapter::plist_reader::SystemPlistReader;
use launchpad::domain::action::TriggerAction;
use launchpad::domain::job::JobSummary;
use launchpad::service::action_service::ActionService;
use launchpad::service::delete_service::DeleteService;
use launchpad::service::job_service::JobService;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use tracing_subscriber::EnvFilter;

use crate::ui_state::UiState;
use crate::MainWindow;

struct AppController {
    job_service: JobService,
    action_service: ActionService,
    delete_service: DeleteService,
    jobs: Vec<JobSummary>,
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
            ui_state: UiState::default(),
        }
    }

    fn refresh(&mut self, ui: &MainWindow) {
        self.ui_state.reset_pending_delete();
        match self.job_service.list_jobs() {
            Ok(jobs) => {
                self.jobs = jobs;
                let list_lines: Vec<SharedString> =
                    self.jobs.iter().map(|job| job.list_line().into()).collect();
                ui.set_job_lines(ModelRc::new(VecModel::from(list_lines)));

                if self.jobs.is_empty() {
                    self.ui_state.clear_selection();
                    self.update_selection_details(ui);
                    ui.set_status_message(
                        "No launchd jobs found in configured directories.".into(),
                    );
                    return;
                }

                let new_selected = self
                    .ui_state
                    .selected_index
                    .filter(|idx| *idx < self.jobs.len())
                    .unwrap_or(0);
                self.ui_state.select(new_selected);
                self.update_selection_details(ui);
                ui.set_status_message(format!("Loaded {} jobs.", self.jobs.len()).into());
            }
            Err(err) => {
                self.jobs.clear();
                self.ui_state.clear_selection();
                ui.set_job_lines(ModelRc::new(VecModel::from(Vec::<SharedString>::new())));
                self.update_selection_details(ui);
                ui.set_status_message(format!("Failed to refresh jobs: {err}").into());
            }
        }
    }

    fn select(&mut self, ui: &MainWindow, index: usize) {
        if index >= self.jobs.len() {
            return;
        }
        self.ui_state.select(index);
        self.update_selection_details(ui);
        ui.set_status_message(format!("Selected {}", self.jobs[index].label).into());
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

        let selected_job = self.jobs[index].clone();
        match self.action_service.execute(&selected_job, action) {
            Ok(()) => {
                ui.set_status_message(format!("{} command sent.", action.as_str()).into());
                self.refresh(ui);
            }
            Err(err) => {
                ui.set_status_message(format!("{} failed: {err}", action.as_str()).into());
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

        if !self.ui_state.request_delete_confirmation(index) {
            ui.set_status_message(
                "Delete requested. Click Delete again to confirm removing the selected plist."
                    .into(),
            );
            return;
        }

        let selected_job = self.jobs[index].clone();
        match self.delete_service.delete(&selected_job) {
            Ok(()) => {
                ui.set_status_message("Job plist deleted successfully.".into());
                self.refresh(ui);
            }
            Err(err) => {
                ui.set_status_message(format!("Delete failed: {err}").into());
            }
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

fn clear_details(ui: &MainWindow) {
    ui.set_detail_label("".into());
    ui.set_detail_scope("".into());
    ui.set_detail_status("".into());
    ui.set_detail_path("".into());
    ui.set_detail_error("".into());
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

    controller.borrow_mut().refresh(&ui);
    ui.run()
}
