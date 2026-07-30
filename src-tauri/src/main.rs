// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if everyfile_lib::diagnostics::run_reset_worker_from_args() {
        return;
    }
    let reset_completion = match everyfile_lib::diagnostics::prepare_reset_completion_from_args() {
        Ok(completion) => completion,
        Err(_) => return,
    };
    everyfile_lib::run_with_reset_completion(reset_completion)
}
