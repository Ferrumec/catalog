use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationStatus {
    Pending,
    Validating,
    Passed,
    Failed,
}

impl ValidationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ValidationStatus::Pending => "pending",
            ValidationStatus::Validating => "validating",
            ValidationStatus::Passed => "passed",
            ValidationStatus::Failed => "failed",
        }
    }
}

impl std::str::FromStr for ValidationStatus {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "validating" => Ok(Self::Validating),
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct Version {
    pub id: Uuid,
    pub project_id: Uuid,
    pub version_tag: String,
    pub artifact_url: String,
    pub artifact_size: i64,
    pub checksum: String,
    pub is_yanked: bool,
    pub yank_reason: Option<String>,
    pub yanked_at: Option<DateTime<Utc>>,
    pub yanked_by: Option<Uuid>,
    pub unyanked_at: Option<DateTime<Utc>>,
    pub unyanked_by: Option<Uuid>,
    pub validation_status: String,
    pub validation_details: Option<serde_json::Value>,
    pub developer_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub validated_at: Option<DateTime<Utc>>,
}

impl Version {
    /// PRJ-021: only a validated, non-yanked version is eligible for new
    /// deployments, test sessions, image downloads, or full downloads.
    /// PRJ-020 already keeps this out of scope for *existing* running
    /// deployments — this only governs starting something new.
    pub fn eligible_for_new_deployment(&self) -> bool {
        !self.is_yanked && self.validation_status == ValidationStatus::Passed.as_str()
    }
}

#[derive(Serialize)]
pub struct VersionDto {
    pub id: Uuid,
    pub project_id: Uuid,
    pub version_tag: String,
    pub artifact_size: i64,
    pub checksum: String,
    pub is_yanked: bool,
    pub yank_reason: Option<String>,
    pub yanked_at: Option<DateTime<Utc>>,
    pub validation_status: String,
    pub validation_details: Option<serde_json::Value>,
    pub developer_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub validated_at: Option<DateTime<Utc>>,
}

impl From<Version> for VersionDto {
    fn from(v: Version) -> Self {
        Self {
            id: v.id,
            project_id: v.project_id,
            version_tag: v.version_tag,
            artifact_size: v.artifact_size,
            checksum: v.checksum,
            is_yanked: v.is_yanked,
            yank_reason: v.yank_reason,
            yanked_at: v.yanked_at,
            validation_status: v.validation_status,
            validation_details: v.validation_details,
            developer_id: v.developer_id,
            created_at: v.created_at,
            validated_at: v.validated_at,
        }
    }
}
