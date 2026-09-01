//! PRJ-007/PRJ-012: reject uploads over the configured size limit.
//!
//! Runs before extraction (see `ValidationPipeline::run`), so it's a
//! plain function rather than a `ValidationStage` impl — at the point
//! it runs there's no `Workspace` yet, since the whole point of this
//! check is to decide whether building one (extracting the archive) is
//! even worth doing.

use super::StageReport;

pub fn check(artifact_size: u64, limit: u64) -> StageReport {
    if artifact_size <= limit {
        StageReport {
            stage: "size_limit",
            passed: true,
            message: format!("{artifact_size} bytes (limit {limit})"),
        }
    } else {
        StageReport {
            stage: "size_limit",
            passed: false,
            message: format!("upload is {artifact_size} bytes, exceeding the {limit}-byte limit"),
        }
    }
}
