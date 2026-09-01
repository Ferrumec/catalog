//! PRJ-013: static security/dependency scanning, flagging known-
//! vulnerable crates. Shells out to `cargo audit` (RustSec advisory
//! database) against the extracted project's `Cargo.lock` when the
//! `real_compile_checks` feature is enabled and `cargo-audit` is on
//! `PATH`; a lock file is required (produced by `CompilationStage`'s
//! `cargo check`, or vendored by the developer), since scanning
//! `Cargo.toml` alone can't see the resolved/transitive graph a real
//! vulnerability scan needs.
//!
//! Degraded mode (feature off, or `cargo-audit` missing): reports a
//! non-blocking warning rather than failing every upload — see
//! README.md "Validation pipeline" for why, and for what a production
//! deployment needs to enable this for real.

use super::{StageReport, ValidationError, ValidationStage, Workspace};
use async_trait::async_trait;

pub struct SecurityScanStage;

#[async_trait]
impl ValidationStage for SecurityScanStage {
    fn name(&self) -> &'static str {
        "security_scan"
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
            let _ = dir;
            return Ok(StageReport {
                stage: self.name(),
                passed: true,
                message: "skipped: real_compile_checks feature is disabled on this host — \
                          PRJ-013 is not enforced by this deployment; enable the feature \
                          (and install cargo-audit) where a scan is required"
                    .into(),
            });
        }

        #[cfg(feature = "real_compile_checks")]
        {
            if which_cargo_audit().is_none() {
                return Ok(StageReport {
                    stage: self.name(),
                    passed: true,
                    message: "cargo-audit not found on PATH — security scan skipped \
                              (non-blocking); install cargo-audit to enforce PRJ-013"
                        .into(),
                });
            }
            if !dir.join("Cargo.lock").is_file() {
                return Ok(StageReport {
                    stage: self.name(),
                    passed: true,
                    message: "no Cargo.lock present — security scan skipped (non-blocking)".into(),
                });
            }

            let output = tokio::process::Command::new("cargo")
                .arg("audit")
                .arg("--json")
                .current_dir(dir)
                .kill_on_drop(true)
                .output()
                .await;

            match output {
                Ok(out) => {
                    // cargo-audit exits non-zero when it finds
                    // advisories, so exit status alone can't
                    // distinguish "found vulnerabilities" from "tool
                    // itself errored" — check the JSON body instead.
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    match serde_json::from_str::<serde_json::Value>(&stdout) {
                        Ok(json) => {
                            let count = json["vulnerabilities"]["count"].as_u64().unwrap_or(0);
                            if count == 0 {
                                Ok(StageReport {
                                    stage: self.name(),
                                    passed: true,
                                    message: "no known-vulnerable crates found".into(),
                                })
                            } else {
                                Ok(StageReport {
                                    stage: self.name(),
                                    passed: false,
                                    message: format!(
                                        "{count} known-vulnerable dependenc{} flagged by cargo-audit",
                                        if count == 1 { "y" } else { "ies" }
                                    ),
                                })
                            }
                        }
                        Err(_) => Ok(StageReport {
                            stage: self.name(),
                            passed: false,
                            message: format!(
                                "cargo-audit did not return parseable output: {}",
                                String::from_utf8_lossy(&out.stderr)
                            ),
                        }),
                    }
                }
                Err(e) => Ok(StageReport {
                    stage: self.name(),
                    passed: false,
                    message: format!("could not invoke cargo-audit: {e}"),
                }),
            }
        }
    }
}

#[cfg(feature = "real_compile_checks")]
fn which_cargo_audit() -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("cargo-audit"))
        .find(|p| p.is_file())
}
