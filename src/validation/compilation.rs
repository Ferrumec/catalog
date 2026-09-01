//! PRJ-010: verify the project compiles in the platform's build
//! environment.
//!
//! Behind the `real_compile_checks` feature (default off — see
//! README.md): shells out to `cargo check` against the extracted
//! project with a bounded timeout. `cargo check` (not `cargo build`) is
//! deliberate — it exercises the full borrow/type checker without
//! producing a binary, which is all this gate needs; producing the
//! actual deployable artifact is Execution's `BuildManager` (SDD
//! §5.4), not Catalog's job.
//!
//! With the feature off, this stage is a documented no-op: catalog
//! hosts without a Rust toolchain and crates.io egress on hand (e.g.
//! most CI/dev sandboxes) shouldn't silently fail every upload.

use super::{StageReport, ValidationError, ValidationStage, Workspace};
use async_trait::async_trait;
use std::time::Duration;

pub struct CompilationStage {
    timeout: Duration,
}

impl CompilationStage {
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

#[async_trait]
impl ValidationStage for CompilationStage {
    fn name(&self) -> &'static str {
        "compilation"
    }

    async fn validate(&self, ws: &Workspace) -> Result<StageReport, ValidationError> {
        let Some(dir) = ws.dir() else {
            return Ok(StageReport {
                stage: self.name(),
                passed: false,
                message: "skipped: archive was not extracted".into(),
            });
        };

        #[cfg(not(feature = "real_compile_checks"))]
        {
            let _ = (dir, self.timeout);
            return Ok(StageReport {
                stage: self.name(),
                passed: true,
                message: "skipped: real_compile_checks feature is disabled on this host \
                          (no build toolchain/crates.io egress assumed) — PRJ-010 is not \
                          enforced by this deployment; enable the feature where a build \
                          environment is available"
                    .into(),
            });
        }

        #[cfg(feature = "real_compile_checks")]
        {
            let mut cmd = tokio::process::Command::new("cargo");
            cmd.arg("check")
                .arg("--offline")
                .current_dir(dir)
                .kill_on_drop(true);

            let output = tokio::time::timeout(self.timeout, cmd.output()).await;
            match output {
                Ok(Ok(out)) if out.status.success() => Ok(StageReport {
                    stage: self.name(),
                    passed: true,
                    message: "cargo check succeeded".into(),
                }),
                Ok(Ok(out)) => Ok(StageReport {
                    stage: self.name(),
                    passed: false,
                    message: format!(
                        "cargo check failed:\n{}",
                        String::from_utf8_lossy(&out.stderr)
                    ),
                }),
                Ok(Err(e)) => Ok(StageReport {
                    stage: self.name(),
                    passed: false,
                    message: format!("could not invoke cargo: {e}"),
                }),
                Err(_) => Ok(StageReport {
                    stage: self.name(),
                    passed: false,
                    message: format!(
                        "cargo check did not complete within {}s",
                        self.timeout.as_secs()
                    ),
                }),
            }
        }
    }
}
