mod cli;
mod config;
mod engine;
mod merge;
mod patterns;
mod state;
mod targets;

use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::run() {
        Ok(code) => code,
        Err(err) => {
            if err.downcast_ref::<config::UsageError>().is_some() {
                eprintln!("error: {err}");
                ExitCode::from(2)
            } else {
                eprintln!("error: {err:#}");
                ExitCode::from(1)
            }
        }
    }
}
