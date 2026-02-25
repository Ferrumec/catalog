use crate::{models::AppState, repositories::ProductRepository, routes};
use actix_web::web::{self, ServiceConfig};
use auth_middleware::Auth;
use ferrumec::Permission;
use sqlx::{Error, Pool, Sqlite, sqlite::SqlitePoolOptions};
use std::env;

#[derive(Clone)]
pub struct CatalogModule {
    state: AppState,
}

#[derive(Clone)]
pub struct Permissions {
    pub create_product: Permission,
}

impl CatalogModule {
    pub async fn new(perms: Vec<Permission>) -> Result<Self, Error> {
        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://catalog.db?mode=rwc".to_string());

        let pool: Pool<Sqlite> = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;
        let repo = ProductRepository::new(pool).await?;
        let state = AppState {
            repo,
            permissions: CatalogModule::set_permissions(perms),
        };
        Ok(Self { state })
    }

    pub fn get_permissions() -> Vec<String> {
        vec!["create_product".to_string()]
    }

    pub fn set_permissions(mut perms: Vec<Permission>) -> Permissions {
        Permissions {
            create_product: perms.pop().unwrap(),
        }
    }

    pub fn config(&self, cfg: &mut ServiceConfig, namespace: &str) {
        cfg.service(
            web::scope(namespace)
                .app_data(web::Data::new(self.state.clone()))
                .service(routes::list_products)
                .service(routes::get_product)
                .service(routes::get_product_by_slug)
                .wrap(Auth)
                .service(routes::create_product),
        );
    }
}
