use crate::handlers::{discovery, projects, versions};
use crate::state::AppState;
use actix_web::web::{self, ServiceConfig};

/// Base paths per SDD §6.1: `/api/v1/projects` and `/api/v1/discovery`.
pub fn config(cfg: &mut ServiceConfig, state: web::Data<AppState>) {
    cfg.app_data(state).service(
        web::scope("/api/v1")
            .service(web::scope("/discovery").route("", web::get().to(discovery::discover)))
            .service(
                web::scope("/projects")
                    .route("", web::post().to(projects::create_project))
                    .route(
                        "/by-slug/{slug}",
                        web::get().to(projects::get_project_by_slug),
                    )
                    .route("/{project_id}", web::get().to(projects::get_project))
                    .route("/{project_id}", web::patch().to(projects::update_metadata))
                    .route("/{project_id}", web::delete().to(projects::delete_project))
                    .route(
                        "/{project_id}/current-version",
                        web::put().to(projects::set_current_version),
                    )
                    // GET (list, unauthenticated-ok) and POST (upload,
                    // authenticated) share this exact path — both must be
                    // registered as routes on the *same* resource
                    // (chained here via two `.route()` calls at the
                    // identical pattern) rather than as two separate
                    // resources/scopes: actix commits to the first
                    // resource whose *path* matches and returns 405 for
                    // an unmatched method rather than falling through to
                    // try a sibling resource registered at the same path.
                    .route(
                        "/{project_id}/versions",
                        web::get().to(versions::list_versions),
                    )
                    .route(
                        "/{project_id}/versions",
                        web::post().to(versions::upload_version),
                    )
                    .route(
                        "/{project_id}/versions/{version_id}",
                        web::get().to(versions::get_version),
                    )
                    .route(
                        "/{project_id}/versions/{version_id}/yank",
                        web::post().to(versions::yank_version),
                    )
                    .route(
                        "/{project_id}/versions/{version_id}/unyank",
                        web::post().to(versions::unyank_version),
                    ),
            ),
    );
}
