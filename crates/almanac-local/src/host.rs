//! Opening the service over a root, and keeping `spaces.toml` (what memoryd's daemon does at
//! start and after each request).

use crate::backend::LocalBackend;
use almanac_core::{ReplicaId, RuleSet, SpaceId, SpaceMeta, VaultKind};
use almanac_service::{
    Backend, Clock, ConfigError, Consolidator, MemoryService, SpacesFile, spaces_from_toml,
    spaces_to_toml,
};
use recall::Embedder;

/// Why the root could not be opened or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LocalError {
    /// `spaces.toml` could not be read or written as a file.
    #[error("spaces.toml: {0}")]
    File(String),
    /// `spaces.toml` is not valid.
    #[error("spaces.toml: {0}")]
    Config(#[from] ConfigError),
    /// A Space with this id is already there.
    #[error("the Space already exists")]
    Exists,
}

/// The service over `backend`, with every Space in the root's `spaces.toml` registered (none
/// when the file is new). No Space opens, and no key is asked for, until the first request.
pub fn open<Clk: Clock, E: Embedder, C: Consolidator>(
    backend: LocalBackend<Clk, E, C>,
    rules: RuleSet,
) -> Result<MemoryService<LocalBackend<Clk, E, C>>, LocalError> {
    let path = backend.dirs().spaces_toml();
    let found = match std::fs::read_to_string(&path) {
        Ok(text) => spaces_from_toml(&text)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SpacesFile::default(),
        Err(e) => return Err(LocalError::File(e.to_string())),
    };
    let service = MemoryService::new(backend, rules);
    found
        .spaces
        .into_iter()
        .for_each(|meta| service.register(meta));
    Ok(service)
}

/// Writes `spaces.toml` from what the service knows (call after creating a Space).
pub fn save_spaces<B: Backend>(
    dirs: &almanac_core::Dirs,
    service: &MemoryService<B>,
) -> Result<(), LocalError> {
    let text = spaces_to_toml(&SpacesFile {
        spaces: service.metas(),
    })?;
    let path = dirs.spaces_toml();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| LocalError::File(e.to_string()))?;
    }
    std::fs::write(path, text).map_err(|e| LocalError::File(e.to_string()))
}

/// Registers a new Space (created now, with a fresh replica id) and saves `spaces.toml`.
pub fn create_space<Clk: Clock, E: Embedder, C: Consolidator>(
    service: &MemoryService<LocalBackend<Clk, E, C>>,
    id: SpaceId,
    vault: VaultKind,
) -> Result<SpaceMeta, LocalError> {
    if service.metas().iter().any(|m| m.id == id) {
        return Err(LocalError::Exists);
    }
    let backend = service.backend();
    let mut replica = [0u8; 16];
    replica.copy_from_slice(&backend.random());
    let meta = SpaceMeta {
        id,
        created: backend.clock().now(),
        replica: ReplicaId(replica),
        vault,
        format: 1,
    };
    service.register(meta.clone());
    save_spaces(backend.dirs(), service)?;
    Ok(meta)
}
