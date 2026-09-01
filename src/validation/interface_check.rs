//! PRJ-011: verify the project declares an entry point compatible with
//! the execution environment (a binary that binds to a
//! platform-assigned port).
//!
//! This is necessarily a heuristic at *upload* time — actually proving
//! the binary binds correctly only happens when Execution runs it
//! (PRJ-011's own parenthetical: "e.g."). What's checked here is the
//! set of things that would make a deploy attempt fail immediately and
//! obviously: a `Cargo.toml`, a binary target, and a `PORT`
//! environment-variable read somewhere in the source (the platform's
//! documented convention for the port it assigns).

use super::{StageReport, ValidationError, ValidationStage, Workspace};
use async_trait::async_trait;
use std::fs;

pub struct InterfaceCheckStage;

#[async_trait]
impl ValidationStage for InterfaceCheckStage {
    fn name(&self) -> &'static str {
        "interface_check"
    }

    async fn validate(&self, ws: &Workspace) -> Result<StageReport, ValidationError> {
        let Some(dir) = ws.dir() else {
            return Ok(StageReport {
                stage: self.name(),
                passed: false,
                message: "skipped: archive was not extracted".into(),
            });
        };

        // Uploads may contain a single top-level directory (common for
        // `tar czf project.tar.gz project/`) — look one level down too.
        let root = find_project_root(dir)?;

        let cargo_toml_path = root.join("Cargo.toml");
        let Ok(cargo_toml) = fs::read_to_string(&cargo_toml_path) else {
            return Ok(StageReport {
                stage: self.name(),
                passed: false,
                message: "no Cargo.toml found at the project root".into(),
            });
        };
        if !cargo_toml.contains("[package]") {
            return Ok(StageReport {
                stage: self.name(),
                passed: false,
                message: "Cargo.toml is missing a [package] section".into(),
            });
        }

        let has_main = root.join("src").join("main.rs").is_file();
        let has_bin_target = cargo_toml.contains("[[bin]]");
        if !has_main && !has_bin_target {
            return Ok(StageReport {
                stage: self.name(),
                passed: false,
                message: "no binary target found (expected src/main.rs or a [[bin]] entry) — \
                          library crates aren't deployable web services"
                    .into(),
            });
        }

        // Soft check: look for the documented `PORT` convention
        // anywhere under src/. A miss doesn't fail validation outright
        // — some frameworks read it indirectly — but it's surfaced so
        // developers whose deploy then fails at runtime have a lead.
        let port_convention_found = source_files(&root)?
            .iter()
            .filter_map(|p| fs::read_to_string(p).ok())
            .any(|src| src.contains("\"PORT\"") || src.contains("'PORT'"));

        Ok(StageReport {
            stage: self.name(),
            passed: true,
            message: if port_convention_found {
                "binary target found; PORT environment variable convention detected".into()
            } else {
                "binary target found; PORT environment variable convention not detected \
                 (non-blocking — verify the app reads its listen port from $PORT)"
                    .into()
            },
        })
    }
}

fn find_project_root(dir: &std::path::Path) -> Result<std::path::PathBuf, ValidationError> {
    if dir.join("Cargo.toml").is_file() {
        return Ok(dir.to_path_buf());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && path.join("Cargo.toml").is_file() {
            return Ok(path);
        }
    }
    Ok(dir.to_path_buf())
}

fn source_files(root: &std::path::Path) -> Result<Vec<std::path::PathBuf>, ValidationError> {
    let mut out = Vec::new();
    let src = root.join("src");
    if !src.is_dir() {
        return Ok(out);
    }
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    Ok(out)
}
