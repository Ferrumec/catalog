use crate::domain::{CatalogError, ProjectDto};
use crate::identity::Identity;
use crate::models::{
    ApiResponse, CreateProjectRequest, SetCurrentVersionRequest, UpdateMetadataRequest,
};
use crate::repository::project_repo::{MetadataUpdate, NewProject};
use crate::state::AppState;
use actix_web::{HttpResponse, web};
use uuid::Uuid;
use validator::Validate;

/// `POST /api/v1/projects` — PRJ-001/PRJ-016.
pub async fn create_project(
    state: web::Data<AppState>,
    identity: Identity,
    req: web::Json<CreateProjectRequest>,
) -> Result<HttpResponse, CatalogError> {
    let req = req.into_inner();
    req.validate()
        .map_err(|e| CatalogError::BadRequest(e.to_string()))?;

    let project = state
        .projects
        .create(NewProject {
            name: req.name,
            description: req.description,
            documentation_url: req.documentation_url,
            owner_id: identity.user_id,
            tags: req.tags,
            categories: req.categories,
        })
        .await?;

    let tags = state.projects.tags_for(project.id).await?;
    let categories = state.projects.categories_for(project.id).await?;
    Ok(HttpResponse::Created().json(ApiResponse::success(
        ProjectDto::from_project(project, tags, categories),
        "Project created",
    )))
}

/// `GET /api/v1/projects/{project_id}` — DSC-002. Reads are
/// unauthenticated (SDD §6.1: "Bearer, optional for reads").
pub async fn get_project(
    state: web::Data<AppState>,
    project_id: web::Path<Uuid>,
) -> Result<HttpResponse, CatalogError> {
    let project = state.projects.get(project_id.into_inner()).await?;
    let tags = state.projects.tags_for(project.id).await?;
    let categories = state.projects.categories_for(project.id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        ProjectDto::from_project(project, tags, categories),
        "OK",
    )))
}

/// `GET /api/v1/projects/by-slug/{slug}` — same as above, resolved by
/// the human-readable slug used in URLs/discovery results.
pub async fn get_project_by_slug(
    state: web::Data<AppState>,
    slug: web::Path<String>,
) -> Result<HttpResponse, CatalogError> {
    let project = state.projects.get_by_slug(&slug.into_inner()).await?;
    let tags = state.projects.tags_for(project.id).await?;
    let categories = state.projects.categories_for(project.id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        ProjectDto::from_project(project, tags, categories),
        "OK",
    )))
}

/// `PATCH /api/v1/projects/{project_id}` — PRJ-017/PRJ-018: metadata
/// updates independent of versions, with history recorded.
pub async fn update_metadata(
    state: web::Data<AppState>,
    identity: Identity,
    project_id: web::Path<Uuid>,
    req: web::Json<UpdateMetadataRequest>,
) -> Result<HttpResponse, CatalogError> {
    let req = req.into_inner();
    req.validate()
        .map_err(|e| CatalogError::BadRequest(e.to_string()))?;
    let project_id = project_id.into_inner();

    let existing = state.projects.get(project_id).await?;
    if existing.owner_id != identity.user_id {
        return Err(CatalogError::NotOwner);
    }

    let project = state
        .projects
        .update_metadata(
            project_id,
            identity.user_id,
            MetadataUpdate {
                name: req.name,
                description: req.description,
                documentation_url: req.documentation_url,
            },
        )
        .await?;

    if let Some(tags) = req.tags {
        state.projects.replace_tags(project_id, &tags).await?;
    }
    if let Some(categories) = req.categories {
        state
            .projects
            .replace_categories(project_id, &categories)
            .await?;
    }

    let tags = state.projects.tags_for(project.id).await?;
    let categories = state.projects.categories_for(project.id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(
        ProjectDto::from_project(project, tags, categories),
        "Project updated",
    )))
}

/// `DELETE /api/v1/projects/{project_id}` — PRJ-005.
pub async fn delete_project(
    state: web::Data<AppState>,
    identity: Identity,
    project_id: web::Path<Uuid>,
) -> Result<HttpResponse, CatalogError> {
    state
        .projects
        .soft_delete(project_id.into_inner(), identity.user_id)
        .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success((), "Project deleted")))
}

/// `PUT /api/v1/projects/{project_id}/current-version` — PRJ-006:
/// point "current" at a version without disturbing anyone pinned to an
/// earlier one.
pub async fn set_current_version(
    state: web::Data<AppState>,
    identity: Identity,
    project_id: web::Path<Uuid>,
    req: web::Json<SetCurrentVersionRequest>,
) -> Result<HttpResponse, CatalogError> {
    let project_id = project_id.into_inner();
    let project = state.projects.get(project_id).await?;
    if project.owner_id != identity.user_id {
        return Err(CatalogError::NotOwner);
    }
    let version = state.versions.get(req.version_id).await?;
    if version.project_id != project_id {
        return Err(CatalogError::BadRequest(
            "version does not belong to this project".into(),
        ));
    }
    state
        .projects
        .set_current_version(project_id, version.id)
        .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success((), "Current version updated")))
}
