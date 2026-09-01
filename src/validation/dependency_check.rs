//! PRJ-012: verify the project doesn't exceed configured dependency
//! limits. RS §15-2 leaves the actual threshold an open item (see
//! `PipelineConfig`) — this stage enforces whatever's configured.
//!
//! Prefers `Cargo.lock` (present after `CompilationStage` runs `cargo
//! check`, or if the developer vendored one) since it counts the full
//! resolved dependency graph, transitive included, which is what
//! actually gets built. Falls back to counting direct entries under
//! `[dependencies]`/`[dev-dependencies]` in `Cargo.toml` when no lock
//! file is available (e.g. `real_compile_checks` is off) — a
//! deliberately conservative undercount, flagged as such in the report.

use super::{StageReport, ValidationError, ValidationStage, Workspace};
use async_trait::async_trait;
use std::fs;
use std::path::Path;

pub struct DependencyCheckStage {
    max_dependencies: usize,
}

impl DependencyCheckStage {
    pub fn new(max_dependencies: usize) -> Self {
        Self { max_dependencies }
    }
}

#[async_trait]
impl ValidationStage for DependencyCheckStage {
    fn name(&self) -> &'static str {
        "dependency_check"
    }

    async fn validate(&self, ws: &Workspace) -> Result<StageReport, ValidationError> {
        let Some(dir) = ws.dir() else {
            return Ok(StageReport {
                stage: self.name(),
                passed: false,
                message: "skipped: archive was not extracted".into(),
            });
        };
        let root = find_project_root(dir)?;

        if let Some(lock) = read_first(&[root.join("Cargo.lock")]) {
            let count = lock.matches("[[package]]").count();
            return Ok(report(
                self.max_dependencies,
                count,
                "Cargo.lock (resolved graph)",
            ));
        }

        let toml = read_first(&[root.join("Cargo.toml")]).unwrap_or_default();
        let count = count_direct_dependencies(&toml);
        Ok(report(
            self.max_dependencies,
            count,
            "Cargo.toml direct dependencies only — no Cargo.lock available, \
             so this undercounts the full resolved graph",
        ))
    }
}

fn report(max: usize, count: usize, source: &str) -> StageReport {
    StageReport {
        stage: "dependency_check",
        passed: count <= max,
        message: format!("{count} dependencies (limit {max}) — counted from {source}"),
    }
}

fn read_first(candidates: &[std::path::PathBuf]) -> Option<String> {
    candidates.iter().find_map(|p| fs::read_to_string(p).ok())
}

fn find_project_root(dir: &Path) -> Result<std::path::PathBuf, ValidationError> {
    if dir.join("Cargo.toml").is_file() {
        return Ok(dir.to_path_buf());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() && path.join("Cargo.toml").is_file() {
            return Ok(path);
        }
    }
    Ok(dir.to_path_buf())
}

/// Very small line-oriented TOML dependency counter — deliberately not
/// a full TOML parse. Counts non-empty, non-comment lines inside
/// `[dependencies]` / `[dev-dependencies]` / `[build-dependencies]`
/// tables until the next `[...]` section header.
fn count_direct_dependencies(cargo_toml: &str) -> usize {
    let dep_sections = [
        "[dependencies]",
        "[dev-dependencies]",
        "[build-dependencies]",
    ];
    let mut count = 0;
    let mut in_section = false;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = dep_sections.contains(&trimmed);
            continue;
        }
        if in_section && !trimmed.is_empty() && !trimmed.starts_with('#') {
            count += 1;
        }
    }
    count
}
