// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if everyfile_lib::diagnostics::run_reset_worker_from_args() {
        return;
    }
    everyfile_lib::run()
}
