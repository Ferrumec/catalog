use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateProjectRequest {
    #[validate(length(min = 3, max = 255))]
    pub name: String,
    pub description: Option<String>,
    #[validate(url)]
    pub documentation_url: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub categories: Vec<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMetadataRequest {
    #[validate(length(min = 3, max = 255))]
    pub name: Option<String>,
    pub description: Option<String>,
    #[validate(url)]
    pub documentation_url: Option<String>,
    pub tags: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct YankRequest {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct SetCurrentVersionRequest {
    pub version_id: uuid::Uuid,
}

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: String,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T, message: impl Into<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: message.into(),
        }
    }
}

impl ApiResponse<()> {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            message: message.into(),
        }
    }
}
