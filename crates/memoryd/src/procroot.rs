//! Where the daemon reads `/proc/<pid>/cgroup`: the system's `/proc`, unless a test build is
//! told otherwise.
//!
//! `MEMORYD_PROC_ROOT=<dir>` makes the daemon read `<dir>/<pid>/cgroup` for its callers, only in
//! a build with the `test-keys` feature (off by default, never in a release or dist build).
//! Without the feature the variable is ignored and the daemon says so, so an environment cannot
//! make a production daemon believe a caller is somebody else. It exists so an acceptance run
//! can present its test processes as `intentd.service` and the like.

use std::path::PathBuf;

/// The environment variable the switch reads.
pub const PROC_ROOT_VAR: &str = "MEMORYD_PROC_ROOT";

/// Whether this build has the fixture `/proc` switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestProcRoot {
    /// Built with the `test-proc-root` feature.
    Built,
    /// The normal build.
    NotBuilt,
}

impl TestProcRoot {
    /// What this build is.
    pub const THIS_BUILD: TestProcRoot = if cfg!(feature = "test-proc-root") {
        TestProcRoot::Built
    } else {
        TestProcRoot::NotBuilt
    };
}

/// The `/proc` the daemon reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcRoot {
    /// The system's `/proc`.
    System,
    /// The system's `/proc`, though the variable was set: this build ignores it.
    SystemIgnoring(String),
    /// A fixture tree (test builds only).
    Fixture(PathBuf),
}

/// Chooses the root, pure over the variable's value and the build.
pub fn proc_root_choice(var: Option<&str>, build: TestProcRoot) -> ProcRoot {
    match (var, build) {
        (None | Some(""), _) => ProcRoot::System,
        (Some(value), TestProcRoot::NotBuilt) => ProcRoot::SystemIgnoring(value.to_owned()),
        (Some(value), TestProcRoot::Built) => ProcRoot::Fixture(PathBuf::from(value)),
    }
}

impl ProcRoot {
    /// The directory to read.
    pub fn path(&self) -> PathBuf {
        match self {
            ProcRoot::System | ProcRoot::SystemIgnoring(_) => PathBuf::from("/proc"),
            ProcRoot::Fixture(dir) => dir.clone(),
        }
    }

    /// What the daemon says on standard error about the choice, if anything.
    pub fn said(&self) -> Option<String> {
        match self {
            ProcRoot::System => None,
            ProcRoot::SystemIgnoring(_) => Some(format!(
                "{PROC_ROOT_VAR} is set but this build has no test-proc-root feature; ignoring it and reading /proc"
            )),
            ProcRoot::Fixture(dir) => Some(format!(
                "TEST BUILD: reading callers from the proc root {}, not /proc",
                dir.display()
            )),
        }
    }
}
