#![forbid(unsafe_code)]
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod comparison;
mod pane_runtime;
mod preferences;
mod raw_pipeline;
mod registration;
mod workspace_batch;

fn main() -> eframe::Result {
    app::run()
}
