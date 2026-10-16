//! The root service, started by its socket unit. Everything real lives in the library.

use std::process::ExitCode;

fn main() -> ExitCode {
    match turnstile::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("turnstile: {e}");
            ExitCode::FAILURE
        }
    }
}
