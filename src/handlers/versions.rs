use crate::domain::version::ValidationStatus;
use crate::domain::{CatalogError, VersionDto};
use crate::events::{VersionPublished, VersionUnyanked, VersionYanked};
use crate::identity::Identity;
use crate::models::{ApiResponse, YankRequest};
use crate::repository::version_repo::NewVersion;
use crate::state::AppState;
use actix_multipart::Multipart;
use actix_web::{HttpMessage, HttpRequest, HttpResponse, web};
use actixutils::middleware::RequestIdStr;
use futures_util::TryStreamExt;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Read the UUID `RequestId` middleware (`actixutils::middleware::
/// RequestId`, applied app-wide in `main.rs`) stashed in the request
/// extensions, for trace correlation on published events. Falls back to
/// a fresh id rather than failing the request if the middleware somehow
/// isn't present or its value isn't a valid UUID — a missing trace id
/// shouldn't block an otherwise-successful upload/yank/un-yank.
fn trace_id(req: &HttpRequest) -> Uuid {
    req.extensions()
        .get::<RequestIdStr>()
        .and_then(|r| Uuid::parse_str(&r.0).ok())
        .unwrap_or_else(Uuid::new_v4)
}

/// `POST /api/v1/projects/{project_id}/versions` — PRJ-001/PRJ-002.
///
/// Multipart body: a `version_tag` text field and an `artifact` file
/// field (a `.tar.gz` of the project). Validation (PRJ-002/010-015)
/// runs synchronously before anything is persisted — PRJ-014 says
/// uploads that fail validation are *rejected*, so a failing upload
/// never becomes a `versions` row at all; the failure report (PRJ-015)
/// comes back in the response body instead.
pub async fn upload_version(
    state: web::Data<AppState>,
    identity: Identity,
    project_id: web::Path<Uuid>,
    mut payload: Multipart,
    req: HttpRequest,
) -> Result<HttpResponse, CatalogError> {
    let project_id = project_id.into_inner();
    let project = state.projects.get(project_id).await?;
    if project.owner_id != identity.user_id {
        return Err(CatalogError::NotOwner);
    }

    let mut version_tag: Option<String> = None;
    let mut artifact_bytes: Option<Vec<u8>> = None;

    while let Some(mut field) = payload
        .try_next()
        .await
        .map_err(|e| CatalogError::BadRequest(format!("malformed multipart body: {e}")))?
    {
        let name = field
            .content_disposition()
            .get_name()
            .unwrap_or("")
            .to_string();
        let mut buf = Vec::new();
        while let Some(chunk) = field
            .try_next()
            .await
            .map_err(|e| CatalogError::BadRequest(format!("error reading upload: {e}")))?
        {
            buf.extend_from_slice(&chunk);
        }
        match name.as_str() {
            "version_tag" => version_tag = Some(String::from_utf8_lossy(&buf).trim().to_string()),
            "artifact" => artifact_bytes = Some(buf),
            _ => {} // ignore unknown fields
        }
    }

    let version_tag = version_tag
        .filter(|t| !t.is_empty())
        .ok_or_else(|| CatalogError::BadRequest("version_tag field is required".into()))?;
    let artifact_bytes = artifact_bytes
        .ok_or_else(|| CatalogError::BadRequest("artifact field (file) is required".into()))?;

    // PRJ-002/PRJ-010–015: run the pipeline before persisting anything.
    let result = state.pipeline.run(&artifact_bytes).await;
    if !result.passed {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "validation failed",
            "report": result,
        })));
    }

    let checksum = {
        let mut hasher = Sha256::new();
        hasher.update(&artifact_bytes);
        format!("{:x}", hasher.finalize())
    };

    // Content-addressed: store before the DB row exists (its id is
    // DB-generated on insert, so it can't be the key) and before we
    // know whether `create` below will succeed — if it fails, this
    // leaves an orphaned blob rather than a DB row with no backing
    // artifact, which is the safer failure mode (a cleanup sweep for
    // unreferenced blobs is cheap; a version row that 404s on download
    // is not).
    let artifact_url = state
        .artifacts
        .put(&checksum, &artifact_bytes)
        .await
        .map_err(|e| CatalogError::Internal(format!("failed to store artifact: {e}")))?;

    let version = state
        .versions
        .create(NewVersion {
            project_id,
            version_tag: version_tag.clone(),
            artifact_url,
            artifact_size: artifact_bytes.len() as i64,
            checksum,
            developer_id: identity.user_id,
        })
        .await?;

    state
        .versions
        .mark_validated(
            version.id,
            ValidationStatus::Passed,
            serde_json::to_value(&result).unwrap_or(serde_json::Value::Null),
        )
        .await?;

    // First accepted version makes the project visible (PRJ-002 gate on
    // publication); later versions don't need to repeat this.
    if !project.is_published {
        state.projects.publish(project_id).await?;
    }
    if project.current_version_id.is_none() {
        state
            .projects
            .set_current_version(project_id, version.id)
            .await?;
    }

    state
        .publish(
            VersionPublished {
                project_id,
                version_id: version.id,
                version_tag,
                developer_id: identity.user_id,
            },
            trace_id(&req),
            identity.user_id,
        )
        .await;

    let version = state.versions.get(version.id).await?;
    Ok(HttpResponse::Created().json(ApiResponse::success(
        VersionDto::from(version),
        "Version published",
    )))
}

/// `GET /api/v1/projects/{project_id}/versions` — PRJ-009.
pub async fn list_versions(
    state: web::Data<AppState>,
    project_id: web::Path<Uuid>,
) -> Result<HttpResponse, CatalogError> {
    let versions = state
        .versions
        .list_for_project(project_id.into_inner())
        .await?;
    let dtos: Vec<VersionDto> = versions.into_iter().map(VersionDto::from).collect();
    Ok(HttpResponse::Ok().json(ApiResponse::success(dtos, "OK")))
}

/// `GET /api/v1/projects/{project_id}/versions/{version_id}` — also
/// doubles as the REST equivalent of the SDD's internal `GetVersion`
/// gRPC call (SDD §6.3/§3.2: Execution → Catalog, yank-status check
/// before provisioning, PRJ-021). A full `tonic`/protobuf service
/// wasn't implemented for this deliverable — see README.md "Open
/// items" — but the response shape below carries exactly what that
/// call needs (`eligible_for_new_deployment`), so wiring the gRPC
/// service later is additive, not a redesign.
pub async fn get_version(
    state: web::Data<AppState>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse, CatalogError> {
    let (project_id, version_id) = path.into_inner();
    let version = state.versions.get(version_id).await?;
    if version.project_id != project_id {
        return Err(CatalogError::VersionNotFound);
    }
    let eligible = version.eligible_for_new_deployment();
    let mut body = serde_json::to_value(VersionDto::from(version)).unwrap_or_default();
    if let serde_json::Value::Object(ref mut map) = body {
        map.insert("eligible_for_new_deployment".into(), eligible.into());
    }
    Ok(HttpResponse::Ok().json(ApiResponse::success(body, "OK")))
}

/// `POST /api/v1/projects/{project_id}/versions/{version_id}/yank` — PRJ-019.
pub async fn yank_version(
    state: web::Data<AppState>,
    identity: Identity,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<YankRequest>,
    req: HttpRequest,
) -> Result<HttpResponse, CatalogError> {
    let (project_id, version_id) = path.into_inner();
    // Check the URL's project_id/version_id pairing *before* mutating —
    // otherwise a mismatched project_id in the URL would still yank a
    // version the caller owns (via its real project) and only report
    // "not found" afterwards, leaving state changed under a response
    // that says nothing happened.
    if state.versions.get(version_id).await?.project_id != project_id {
        return Err(CatalogError::VersionNotFound);
    }
    let version = state
        .versions
        .yank(
            version_id,
            identity.user_id,
            identity.user_id,
            body.into_inner().reason,
        )
        .await?;

    state
        .publish(
            VersionYanked {
                project_id,
                version_id: version.id,
                version_tag: version.version_tag.clone(),
                developer_id: identity.user_id,
                reason: version.yank_reason.clone(),
            },
            trace_id(&req),
            identity.user_id,
        )
        .await;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        VersionDto::from(version),
        "Version yanked",
    )))
}

/// `POST /api/v1/projects/{project_id}/versions/{version_id}/unyank` — PRJ-022.
pub async fn unyank_version(
    state: web::Data<AppState>,
    identity: Identity,
    path: web::Path<(Uuid, Uuid)>,
    req: HttpRequest,
) -> Result<HttpResponse, CatalogError> {
    let (project_id, version_id) = path.into_inner();
    if state.versions.get(version_id).await?.project_id != project_id {
        return Err(CatalogError::VersionNotFound);
    }
    let version = state
        .versions
        .unyank(version_id, identity.user_id, identity.user_id)
        .await?;

    state
        .publish(
            VersionUnyanked {
                project_id,
                version_id: version.id,
                version_tag: version.version_tag.clone(),
                developer_id: identity.user_id,
            },
            trace_id(&req),
            identity.user_id,
        )
        .await;

    Ok(HttpResponse::Ok().json(ApiResponse::success(
        VersionDto::from(version),
        "Version un-yanked",
    )))
}
