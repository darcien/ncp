//! The `ncp` command line, built in the test kit. Cargo gives the tests of
//! a package only the paths of the binaries of that package. This binary
//! runs the same code as `ncp`, with one change: the native store is the
//! fake `security` program at the path in `NCP_TEST_SECURITY_PROGRAM`, or
//! no store when that variable is not set. Thus a test never uses the real
//! Keychain or Secret Service.

use std::{sync::Arc, time::Duration};

use ncp::store::{CredentialStore, SecurityStore, StoreError};

fn fake_native(limit: Duration) -> Result<Arc<dyn CredentialStore>, StoreError> {
    match std::env::var_os("NCP_TEST_SECURITY_PROGRAM") {
        Some(program) => Ok(Arc::new(
            SecurityStore::with_program(program).with_limit(limit),
        )),
        None => Err(StoreError::Unavailable(
            "the test has no native store".into(),
        )),
    }
}

fn main() -> std::process::ExitCode {
    ncp::cli::main_with(fake_native)
}
