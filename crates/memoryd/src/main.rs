//! memoryd: serves `org.quire.Memory1` over almanac-service. A skeleton: it resolves its
//! directories, says what is frozen and exits.

use clap::Parser;
use memoryd::{MEMORY_BUS, dirs_from_env};
use std::process::ExitCode;

/// almanac's memory daemon (`org.quire.Memory1`).
#[derive(Debug, Parser)]
#[command(name = "memoryd", version)]
struct Args {}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let Args {} = Args::parse();
    // The daemon's one log path is standard error, prefixed with its name.
    match dirs_from_env() {
        Ok(dirs) => eprintln!(
            "memoryd: not implemented: {MEMORY_BUS} is frozen as an interface; memory would live in {}",
            dirs.memory().display()
        ),
        Err(why) => {
            eprintln!("memoryd: not implemented: {MEMORY_BUS} is frozen as an interface ({why})")
        }
    }
    ExitCode::from(2)
}
