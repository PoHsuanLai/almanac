//! Which key store the daemon uses: the Secret Service, unless a test build is told otherwise.
//!
//! `MEMORYD_KEYS=file:<path>` selects a file-backed store, but only in a build with the
//! `test-keys` feature (off by default, never in a release or dist build). Without the feature
//! the variable is ignored, and the daemon says so on standard error when it is set: there is no
//! way to downgrade a production daemon's keys by environment.

use almanac_core::SpaceId;
use almanac_seal::{KeyError, KeyStore, Oo7Keys, SpaceKey};
use std::path::PathBuf;

/// The environment variable the switch reads.
pub const KEYS_VAR: &str = "MEMORYD_KEYS";

/// Whether this build has the test key store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestKeys {
    /// Built with the `test-keys` feature.
    Built,
    /// The normal build.
    NotBuilt,
}

impl TestKeys {
    /// What this build is.
    pub const THIS_BUILD: TestKeys = if cfg!(feature = "test-keys") {
        TestKeys::Built
    } else {
        TestKeys::NotBuilt
    };
}

/// The choice, pure over the variable's value and the build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// The Secret Service (the variable is unset).
    SecretService,
    /// The Secret Service, though the variable was set: this build ignores it.
    SecretServiceIgnoring(String),
    /// The sealed test file at this path.
    File(PathBuf),
    /// The variable is set in a test build but is not `file:<path>`.
    Unreadable(String),
}

/// Chooses the key store.
pub fn select(var: Option<&str>, build: TestKeys) -> Selection {
    match (var, build) {
        (None, _) => Selection::SecretService,
        (Some(value), TestKeys::NotBuilt) => Selection::SecretServiceIgnoring(value.to_owned()),
        (Some(value), TestKeys::Built) => match value.strip_prefix("file:") {
            Some(path) if !path.is_empty() => Selection::File(PathBuf::from(path)),
            _ => Selection::Unreadable(value.to_owned()),
        },
    }
}

impl Selection {
    /// The directory the sandbox must let the daemon write for this choice: the key file's.
    pub fn writable_dir(&self) -> Option<PathBuf> {
        match self {
            Selection::File(path) => path.parent().map(PathBuf::from),
            Selection::SecretService
            | Selection::SecretServiceIgnoring(_)
            | Selection::Unreadable(_) => None,
        }
    }
}

/// The environment variable that turns the Landlock sandbox off, in a test build only.
pub const SANDBOX_VAR: &str = "MEMORYD_SANDBOX";

/// Whether the daemon applies its Landlock sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sandbox {
    /// Applied (always, in a normal build).
    On,
    /// Not applied: a test build was told `MEMORYD_SANDBOX=off`. A Landlocked process may not
    /// read `/proc/<pid>/exe` of a process outside its domain (the kernel's ptrace check), so
    /// the daemon cannot tell its callers apart under it; the jail is the isolation then.
    Off,
}

/// Chooses the sandbox, pure over the variable's value and the build.
pub fn sandbox_choice(var: Option<&str>, build: TestKeys) -> Sandbox {
    match (var, build) {
        (Some("off"), TestKeys::Built) => Sandbox::Off,
        _ => Sandbox::On,
    }
}

/// The key store the daemon runs with.
#[derive(Debug)]
pub enum AnyKeys {
    /// The session's Secret Service.
    Oo7(Oo7Keys),
    /// A sealed file (test builds only).
    #[cfg(feature = "test-keys")]
    File(almanac_seal::FileKeys),
}

impl AnyKeys {
    /// The store `selection` names, and the line the daemon says about it, if any.
    pub fn from_selection(selection: Selection) -> Result<(Self, Option<String>), String> {
        match selection {
            Selection::SecretService => Ok((Self::Oo7(Oo7Keys), None)),
            Selection::SecretServiceIgnoring(_) => Ok((
                Self::Oo7(Oo7Keys),
                Some(format!(
                    "{KEYS_VAR} is set but this build has no test-keys feature; ignoring it and using the Secret Service"
                )),
            )),
            Selection::Unreadable(value) => Err(format!("{KEYS_VAR}={value} is not file:<path>")),
            #[cfg(feature = "test-keys")]
            Selection::File(path) => Ok((
                Self::File(almanac_seal::FileKeys::at(&path)),
                Some(format!(
                    "TEST BUILD: keys are in the file {}, not the Secret Service",
                    path.display()
                )),
            )),
            #[cfg(not(feature = "test-keys"))]
            Selection::File(_) => Err("a file key store needs the test-keys feature".to_owned()),
        }
    }
}

impl KeyStore for AnyKeys {
    async fn get(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        match self {
            Self::Oo7(keys) => keys.get(space).await,
            #[cfg(feature = "test-keys")]
            Self::File(keys) => keys.get(space).await,
        }
    }

    async fn create(&self, space: &SpaceId) -> Result<SpaceKey, KeyError> {
        match self {
            Self::Oo7(keys) => keys.create(space).await,
            #[cfg(feature = "test-keys")]
            Self::File(keys) => keys.create(space).await,
        }
    }

    async fn destroy(&self, space: &SpaceId) -> Result<(), KeyError> {
        match self {
            Self::Oo7(keys) => keys.destroy(space).await,
            #[cfg(feature = "test-keys")]
            Self::File(keys) => keys.destroy(space).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_is_the_secret_service_in_every_build() {
        for build in [TestKeys::Built, TestKeys::NotBuilt] {
            assert_eq!(select(None, build), Selection::SecretService);
        }
    }

    #[test]
    fn a_normal_build_ignores_the_variable() {
        assert_eq!(
            select(Some("file:/x/keys"), TestKeys::NotBuilt),
            Selection::SecretServiceIgnoring("file:/x/keys".to_owned())
        );
        let (keys, said) = AnyKeys::from_selection(select(Some("file:/x"), TestKeys::NotBuilt))
            .expect("ignored, not an error");
        assert!(matches!(keys, AnyKeys::Oo7(_)));
        assert!(said.is_some_and(|line| line.contains("ignoring")));
    }

    #[test]
    fn a_test_build_reads_file_paths_only() {
        let cases = [
            ("file:/x/keys", Selection::File(PathBuf::from("/x/keys"))),
            ("file:", Selection::Unreadable("file:".to_owned())),
            ("/x/keys", Selection::Unreadable("/x/keys".to_owned())),
        ];
        for (value, want) in cases {
            assert_eq!(select(Some(value), TestKeys::Built), want, "{value}");
        }
    }

    /// The build under test: the default build ignores the variable, the feature build obeys it.
    #[test]
    fn only_a_file_choice_needs_its_directory_writable() {
        let file = select(Some("file:/x/keys"), TestKeys::Built);
        assert_eq!(file.writable_dir(), Some(PathBuf::from("/x")));
        assert_eq!(Selection::SecretService.writable_dir(), None);
    }

    #[test]
    fn the_sandbox_goes_off_only_in_a_test_build_told_so() {
        assert_eq!(sandbox_choice(Some("off"), TestKeys::Built), Sandbox::Off);
        assert_eq!(sandbox_choice(Some("off"), TestKeys::NotBuilt), Sandbox::On);
        assert_eq!(sandbox_choice(Some("on"), TestKeys::Built), Sandbox::On);
        assert_eq!(sandbox_choice(None, TestKeys::Built), Sandbox::On);
    }

    #[test]
    fn this_build_is_what_its_features_say() {
        let chosen = select(Some("file:/x/keys"), TestKeys::THIS_BUILD);
        if cfg!(feature = "test-keys") {
            assert_eq!(chosen, Selection::File(PathBuf::from("/x/keys")));
        } else {
            assert!(matches!(chosen, Selection::SecretServiceIgnoring(_)));
        }
    }
}
