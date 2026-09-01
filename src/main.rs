mod artifact_store;
mod config;
mod domain;
mod events;
mod handlers;
mod identity;
mod models;
mod repository;
mod search;
mod state;
mod validation;

use actix_web::{App, HttpServer, web};
use actixutils::middleware::RequestId;
use artifact_store::LocalFsStore;
use sqlx::PgPool;
use state::AppState;
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};
use typed_eventbus::EventStream;
use validation::PipelineConfig;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let db_url =
        std::env::var("CATALOG_DATABASE_URL").expect("var CATALOG_DATABASE_URL not provided");
    let pool = PgPool::connect(&db_url)
        .await
        .expect("could not connect to catalog_db");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("catalog_db migrations failed");

    let artifact_root =
        std::env::var("CATALOG_ARTIFACT_ROOT").unwrap_or_else(|_| "./artifacts".into());
    let artifacts =
        Arc::new(LocalFsStore::new(artifact_root).expect("could not initialize artifact storage"));

    let pipeline_config = PipelineConfig {
        max_artifact_bytes: env_u64("CATALOG_MAX_ARTIFACT_BYTES", 100 * 1024 * 1024),
        max_dependencies: env_u64("CATALOG_MAX_DEPENDENCIES", 500) as usize,
        compile_timeout_secs: env_u64("CATALOG_COMPILE_TIMEOUT_SECS", 120),
    };

    let events = build_event_stream().await;

    let state = web::Data::new(AppState::new(pool, artifacts, pipeline_config, events));

    let bind_addr = std::env::var("CATALOG_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8081".into());
    tracing::info!("catalog service listening on {bind_addr}");

    HttpServer::new(move || {
        App::new()
            // Stamps X-Request-Id and makes it available in extensions
            // (as `actixutils::middleware::RequestIdStr`) for trace
            // correlation on published events — see
            // `handlers/versions.rs::trace_id`. Safe to apply app-wide:
            // unlike `ReadContext`, `RequestId` has no auth dependency
            // and never fails a request.
            .wrap(RequestId)
            .configure(|cfg| config::config(cfg, state.clone()))
    })
    .bind(&bind_addr)?
    .run()
    .await
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Build the NATS-backed `EventStream` handle used to publish
/// `version.published`/`yanked`/`unyanked` (SDD §6.5).
///
/// **Unverified against the real `typed-eventbus` crate** — only
/// `actixutils` (which merely holds an `Arc<dyn EventStream>`, never
/// constructs one) was available while writing this service, not
/// `typed-eventbus` itself. `EventStream` connect/construction here is
/// a best-effort placeholder; if this doesn't compile against your
/// actual `typed-eventbus` version, sharing that crate's source (the
/// way `actixutils.zip` was shared to fix the `Context` wiring) will
/// let this be corrected precisely rather than guessed twice.
async fn build_event_stream() -> Arc<dyn EventStream> {
    let nats_url =
        std::env::var("CATALOG_NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
    Arc::new(
        typed_eventbus::NatsEventStream::new(&nats_url)
            .await
            .expect("could not connect to NATS event bus"),
    )
}
