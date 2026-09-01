//! Upload validation pipeline (PRJ-002, PRJ-010–015).
//!
//! Adapted from the `ValidationPipeline`/`ValidationStage` skeleton in
//! SDD §5.2. Two adaptations from that skeleton, both because the SDD's
//! version is illustrative rather than literal:
//! - `validate` is `async` and takes a shared [`Workspace`] (extracted
//!   project directory + known artifact size) rather than a bare
//!   `Artifact`, since most stages need the archive on disk, not just
//!   its bytes.
//! - [`SizeLimitStage`] still runs first and, on failure, the pipeline
//!   skips extraction entirely rather than decompressing an
//!   already-rejected (and potentially hostile — zip/tar-bomb) upload.

pub mod compilation;
pub mod dependency_check;
pub mod interface_check;
pub mod security_scan;
pub mod size_limit;

use async_trait::async_trait;
use serde::Serialize;
use std::path::Path;
use tempfile::TempDir;

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct StageReport {
    pub stage: &'static str,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ValidationResult {
    pub passed: bool,
    pub reports: Vec<StageReport>,
}

/// What every stage after the size gate operates on: the extracted
/// project directory (`None` if extraction never happened or failed —
/// stages that need it report a failure rather than panicking) plus the
/// raw artifact size, which stages like dependency counting don't need
/// to re-derive.
pub struct Workspace {
    pub artifact_size: u64,
    extracted: Option<TempDir>,
}

impl Workspace {
    pub fn dir(&self) -> Option<&Path> {
        self.extracted.as_ref().map(|t| t.path())
    }
}

#[async_trait]
pub trait ValidationStage: Send + Sync {
    fn name(&self) -> &'static str;
    async fn validate(&self, ws: &Workspace) -> Result<StageReport, ValidationError>;
}

#[derive(Clone)]
pub struct PipelineConfig {
    /// PRJ-007/012. RS §15-2 (SDD Appendix B) leaves the real number an
    /// open item pending resolution — this is the SDD §5.2 default
    /// (100MB), overridable via `CATALOG_MAX_ARTIFACT_BYTES`.
    pub max_artifact_bytes: u64,
    /// PRJ-012 dependency-count ceiling. Also open per RS §15-2;
    /// overridable via `CATALOG_MAX_DEPENDENCIES`.
    pub max_dependencies: usize,
    pub compile_timeout_secs: u64,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            max_artifact_bytes: 100 * 1024 * 1024,
            max_dependencies: 500,
            compile_timeout_secs: 120,
        }
    }
}

pub struct ValidationPipeline {
    config: PipelineConfig,
    stages: Vec<Box<dyn ValidationStage>>,
}

impl ValidationPipeline {
    pub fn new(config: PipelineConfig) -> Self {
        let stages: Vec<Box<dyn ValidationStage>> = vec![
            Box::new(interface_check::InterfaceCheckStage),
            Box::new(compilation::CompilationStage::new(
                config.compile_timeout_secs,
            )),
            Box::new(dependency_check::DependencyCheckStage::new(
                config.max_dependencies,
            )),
            Box::new(security_scan::SecurityScanStage),
        ];
        Self { config, stages }
    }

    /// PRJ-002/PRJ-014: run every stage; PRJ-015: collect one report per
    /// stage so a rejection is actionable rather than a bare failure.
    /// Stages after the size gate still all run even once one fails —
    /// a developer fixing one problem shouldn't have to resubmit
    /// repeatedly to discover the next one.
    pub async fn run(&self, artifact_bytes: &[u8]) -> ValidationResult {
        let size_report =
            size_limit::check(artifact_bytes.len() as u64, self.config.max_artifact_bytes);
        let mut passed = size_report.passed;
        let mut reports = vec![size_report];

        let extracted = if passed {
            match extract_tar_gz(artifact_bytes) {
                Ok(dir) => Some(dir),
                Err(e) => {
                    reports.push(StageReport {
                        stage: "extraction",
                        passed: false,
                        message: format!("could not extract archive as a gzipped tarball: {e}"),
                    });
                    passed = false;
                    None
                }
            }
        } else {
            None
        };

        let ws = Workspace {
            artifact_size: artifact_bytes.len() as u64,
            extracted,
        };

        for stage in &self.stages {
            let report = match stage.validate(&ws).await {
                Ok(r) => r,
                Err(e) => StageReport {
                    stage: stage.name(),
                    passed: false,
                    message: format!("stage error: {e}"),
                },
            };
            if !report.passed {
                passed = false;
            }
            reports.push(report);
        }

        ValidationResult { passed, reports }
    }
}

/// Extract a `.tar.gz` upload into a fresh temp directory. Bounded by
/// `tar`'s normal entry-by-entry streaming (no single call inflates the
/// whole archive into memory), and every extraction target path is
/// validated by the `tar` crate to stay within `dir` (rejecting
/// `..`-escaping entries) before anything is written.
fn extract_tar_gz(bytes: &[u8]) -> Result<TempDir, std::io::Error> {
    let dir = tempfile::tempdir()?;
    let gz = flate2::read::GzDecoder::new(bytes);
    let mut archive = tar::Archive::new(gz);
    archive.set_preserve_permissions(false);
    archive.unpack(dir.path())?;
    Ok(dir)
}
