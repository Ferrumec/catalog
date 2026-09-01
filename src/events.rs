//! Domain events published to the NATS event bus (SDD §6.5, §3.2).
//!
//! Catalog is a *publisher* of these three subjects only — it doesn't
//! subscribe to anything to do its own job (the dependency matrix in
//! SDD §3.2.1 shows Catalog's only inbound coupling is the synchronous
//! `GetVersion` call from Execution, which is a plain read, not an
//! event subscription). Search-index maintenance (mentioned in the
//! diagram as "version events → search index update") is a no-op here
//! because Catalog *is* the source of the `search_vector` column
//! (kept current by a DB trigger — see the migration) rather than a
//! separate index that needs updating from its own events.

use serde::Serialize;
use typed_eventbus::Publishable;
use uuid::Uuid;

#[derive(Serialize)]
pub struct VersionPublished {
    pub project_id: Uuid,
    pub version_id: Uuid,
    pub version_tag: String,
    pub developer_id: Uuid,
}

impl Publishable for VersionPublished {
    const SUBJECT: &'static str = "version.published";
}

#[derive(Serialize)]
pub struct VersionYanked {
    pub project_id: Uuid,
    pub version_id: Uuid,
    pub version_tag: String,
    pub developer_id: Uuid,
    pub reason: Option<String>,
}

impl Publishable for VersionYanked {
    const SUBJECT: &'static str = "version.yanked";
}

#[derive(Serialize)]
pub struct VersionUnyanked {
    pub project_id: Uuid,
    pub version_id: Uuid,
    pub version_tag: String,
    pub developer_id: Uuid,
}

impl Publishable for VersionUnyanked {
    const SUBJECT: &'static str = "version.unyanked";
}
