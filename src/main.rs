slint::include_modules!();

mod app;
mod ui_state;

fn main() -> Result<(), slint::PlatformError> {
    app::run()
}
