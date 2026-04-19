use crate::{
    models::{AppState, Caches, Config, DefaultOnCreate},
    repositories::ProductRepository,
    routes,
    service::Service,
};
use actix_web::web::{self, ServiceConfig};
use ferrumec::crypto::Validate;
use ferrumec::middleware::Auth;
use event_stream::EventStream;
use ferrumec::Permission;
use serde::Deserialize;
use sqlx::{Error, Pool, Sqlite, sqlite::SqlitePoolOptions};
use std::{env, sync::Arc};
use tera::Tera;

#[derive(Clone)]
pub struct CatalogModule {
    state: web::Data<AppState>,
    validator: Arc<dyn Validate>,
}

#[derive(Clone)]
pub struct Permissions {
    pub create_product: Permission,
}

#[derive(Deserialize)]
struct PermissionSet {
    catalog: Vec<Permission>,
}

impl CatalogModule {
    pub fn get_permissions() -> Vec<String> {
        vec!["create_product".to_string()]
    }

    pub fn set_permissions(mut perms: Vec<Permission>) -> Permissions {
        Permissions {
            create_product: perms.pop().unwrap(),
        }
    }

    pub async fn new(
        es: Arc<dyn EventStream>,
        validator: Arc<dyn Validate>,
        pool: Pool<Sqlite>,
        perms: ferrumec::deps::Permissions,
    ) -> Result<CatalogModule, Error> {
        let repo = ProductRepository::new(pool).await?;
        let perms: PermissionSet = serde_json::from_str(perms.0.as_str()).unwrap();
        let state = AppState {
            tera: Tera::new("templates/**/*").unwrap(),
            service: Service::new(repo.clone()),
            caches: Caches::new(),
            repo,
            permissions: Self::set_permissions(perms.catalog),
            es,
        };
        Ok(Self {
            state: web::Data::new(state),
            validator,
        })
    }

    pub fn config(&self, cfg: &mut ServiceConfig, namespace: &str) {
        cfg.service(
            web::scope(namespace)
                .app_data(self.state.clone())
                .service(routes::index)
                .service(routes::list_products)
                .service(routes::get_product)
                .service(routes::get_product_by_slug)
                .service(
                    web::scope("") // same prefix as parent
                        .wrap(Auth {
                            validator: self.validator.clone(),
                        })
                        .service(routes::create_product)
                        .service(routes::update),
                ),
        );
    }
}
