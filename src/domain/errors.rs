use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("project not found")]
    ProjectNotFound,
    #[error("version not found")]
    VersionNotFound,
    #[error("a project with that slug already exists")]
    SlugTaken,
    #[error("a version with that tag already exists for this project")]
    VersionTagTaken,
    #[error("not the owner of this project")]
    NotOwner,
    #[error("version is already yanked")]
    AlreadyYanked,
    #[error("version was never yanked")]
    NotYanked,
    #[error("validation failed: {0}")]
    ValidationFailed(String),
    #[error("upload exceeds the configured size limit ({limit_bytes} bytes)")]
    TooLarge { limit_bytes: u64 },
    #[error("invalid request: {0}")]
    BadRequest(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl ResponseError for CatalogError {
    fn status_code(&self) -> StatusCode {
        match self {
            CatalogError::ProjectNotFound | CatalogError::VersionNotFound => StatusCode::NOT_FOUND,
            CatalogError::SlugTaken | CatalogError::VersionTagTaken => StatusCode::CONFLICT,
            CatalogError::NotOwner => StatusCode::FORBIDDEN,
            CatalogError::AlreadyYanked | CatalogError::NotYanked => StatusCode::CONFLICT,
            CatalogError::ValidationFailed(_) | CatalogError::BadRequest(_) => {
                StatusCode::BAD_REQUEST
            }
            CatalogError::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            CatalogError::Database(_) | CatalogError::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    fn error_response(&self) -> HttpResponse {
        if matches!(self, CatalogError::Database(_) | CatalogError::Internal(_)) {
            tracing::error!("catalog internal error: {self}");
            return HttpResponse::build(self.status_code()).json(ErrorBody {
                error: "internal error".into(),
            });
        }
        HttpResponse::build(self.status_code()).json(ErrorBody {
            error: self.to_string(),
        })
    }
}
