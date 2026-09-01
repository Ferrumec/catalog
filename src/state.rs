use crate::artifact_store::ArtifactStore;
use crate::repository::{ProjectRepo, VersionRepo};
use crate::validation::{PipelineConfig, ValidationPipeline};
use sqlx::PgPool;
use std::sync::Arc;
use typed_eventbus::{Event, EventStream, Publishable};
use uuid::Uuid;

pub struct AppState {
    pub pool: PgPool,
    pub projects: ProjectRepo,
    pub versions: VersionRepo,
    pub artifacts: Arc<dyn ArtifactStore>,
    pub pipeline: Arc<ValidationPipeline>,
    events: Arc<dyn EventStream>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        artifacts: Arc<dyn ArtifactStore>,
        pipeline_config: PipelineConfig,
        events: Arc<dyn EventStream>,
    ) -> Self {
        Self {
            projects: ProjectRepo::new(pool.clone()),
            versions: VersionRepo::new(pool.clone()),
            pipeline: Arc::new(ValidationPipeline::new(pipeline_config)),
            pool,
            artifacts,
            events,
        }
    }

    /// Publish a domain event (SDD §6.5: `version.published` /
    /// `version.yanked` / `version.unyanked`).
    ///
    /// `authnz` builds its event-publishing calls around
    /// `actixutils::locals::Context`, populated per-request by
    /// `actixutils::middleware::ReadContext<T>`. That middleware requires
    /// a `T: GetId` to already be in the request's extensions — i.e. the
    /// whole wrapped resource must be authenticated — which doesn't fit
    /// `/projects/{id}/versions`: that path serves both an authenticated
    /// `POST` (upload) and an unauthenticated `GET` (list, SDD §6.1:
    /// "Bearer, optional for reads"). Wrapping the resource would 500 the
    /// GET; actix also doesn't support registering two separate resources
    /// for the identical path split by method. So instead of going
    /// through `Context`, this publishes straight to the `EventStream`
    /// using the same `typed_eventbus::Event` builder calls `Context::
    /// publish` uses internally (producer name, trace id, acting user) —
    /// same on-the-wire event, no request-extensions dependency.
    pub async fn publish<T: Publishable + Send + Sync>(
        &self,
        payload: T,
        trace_id: Uuid,
        user_id: Uuid,
    ) {
        let event = Event::new(payload)
            .with_producer("catalog".to_string())
            .with_trace_id(trace_id)
            .with_user_id(user_id);
        if let Err(e) = event.publish(self.events.clone()).await {
            tracing::error!(error = %e, %trace_id, "event publish failed");
        }
    }
}
