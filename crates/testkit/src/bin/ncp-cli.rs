//! The `ncp` command line, built in the test kit. Cargo gives the tests of
//! a package only the paths of the binaries of that package. This binary
//! runs the same code as `ncp`.

fn main() -> std::process::ExitCode {
    ncp::cli::main()
}
