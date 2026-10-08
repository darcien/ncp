//! A program that leaves a store call blocked at its exit, with the
//! runtime of the command line. A test measures its exit time.

use std::{process::ExitCode, time::Duration};

fn main() -> ExitCode {
    let Ok(runtime) = ncp::cli::runtime() else {
        return ExitCode::FAILURE;
    };
    // The call passes its limit and stays on its blocking thread, as a store
    // call does when it waits for an unlock prompt.
    let result = runtime.block_on(ncp::store::run_blocking(Duration::from_millis(100), || {
        std::thread::sleep(Duration::from_secs(30));
        Ok(())
    }));
    ncp::cli::shut_down(runtime);
    if result.is_err() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
