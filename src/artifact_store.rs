//! PRJ-003: durable storage for uploaded artifacts.
//!
//! SDD §9 specifies S3-compatible object storage in production. That
//! wiring (e.g. via an S3 SDK client) is a deployment concern this
//! trait exists to keep swappable — [`LocalFsStore`] here is a real,
//! working implementation suitable for local/dev/single-node use, not
//! a stub; a production deployment provides an `ArtifactStore` backed
//! by object storage instead and nothing else in the service changes.

use async_trait::async_trait;
use std::path::PathBuf;

#[async_trait]
pub trait ArtifactStore: Send + Sync {
    /// Store `bytes` under `key` and return a URL/reference suitable
    /// for `versions.artifact_url`. Callers key by the artifact's
    /// SHA-256 checksum (content-addressed) rather than the version
    /// row's id — the row doesn't exist yet when the artifact needs to
    /// be stored (its id is DB-generated on insert), and content
    /// addressing has the side benefit of de-duplicating byte-identical
    /// re-uploads for free.
    async fn put(&self, key: &str, bytes: &[u8]) -> std::io::Result<String>;
    async fn get(&self, artifact_url: &str) -> std::io::Result<Vec<u8>>;
}

pub struct LocalFsStore {
    root: PathBuf,
}

impl LocalFsStore {
    pub fn new(root: impl Into<PathBuf>) -> std::io::Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
}

#[async_trait]
impl ArtifactStore for LocalFsStore {
    async fn put(&self, key: &str, bytes: &[u8]) -> std::io::Result<String> {
        let path = self.root.join(format!("{key}.tar.gz"));
        tokio::fs::write(&path, bytes).await?;
        Ok(format!("file://{}", path.display()))
    }

    async fn get(&self, artifact_url: &str) -> std::io::Result<Vec<u8>> {
        let path = artifact_url.strip_prefix("file://").unwrap_or(artifact_url);
        tokio::fs::read(path).await
    }
}
