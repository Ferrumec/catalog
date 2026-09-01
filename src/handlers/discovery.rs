use crate::domain::CatalogError;
use crate::models::ApiResponse;
use crate::search::{self, DiscoveryQuery};
use crate::state::AppState;
use actix_web::{HttpResponse, web};

/// `GET /api/v1/discovery` — DSC-001/003–009. Unauthenticated (SDD §6.1).
pub async fn discover(
    state: web::Data<AppState>,
    query: web::Query<DiscoveryQuery>,
) -> Result<HttpResponse, CatalogError> {
    let page = search::discover(&state.pool, query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(page, "OK")))
}
