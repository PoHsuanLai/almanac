//! memoryd: serves `org.quire.Memory1` on the session bus over almanac-service. The start-up is
//! `memoryd::start`; this binary reads the arguments and passes it the process environment.

use clap::Parser;
use std::process::ExitCode;

/// almanac's memory daemon (`org.quire.Memory1`).
#[derive(Debug, Parser)]
#[command(name = "memoryd", version)]
struct Args {
    /// Write the settings schema (`almanac.settings.toml`) into this directory and stop.
    #[arg(long, value_name = "DIR")]
    write_schema: Option<std::path::PathBuf>,
}

fn main() -> ExitCode {
    let Args { write_schema } = Args::parse();
    if let Some(dir) = write_schema {
        let written = std::fs::create_dir_all(&dir).and_then(|()| {
            std::fs::write(dir.join("almanac.settings.toml"), almanac_service::SCHEMA)
        });
        return match written {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => {
                eprintln!("memoryd: {}: {why}", dir.display());
                ExitCode::from(1)
            }
        };
    }
    // The daemon's one log path is standard error, prefixed with its name.
    match memoryd::start(&|name| std::env::var(name).ok()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("memoryd: {why}");
            ExitCode::from(1)
        }
    }
}
