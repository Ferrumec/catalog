use crate::{
    models::{AppState, Caches, Config, DefaultOnCreate},
    repositories::ProductRepository,
    routes,
    service::Service,
};
use actix_web::web::{self, ServiceConfig};
use auth_middleware::Auth;
use ferrumec::Permission;
use sqlx::{Error, Pool, Sqlite, sqlite::SqlitePoolOptions};
use std::env;
use tera::Tera;

#[derive(Clone)]
pub struct CatalogModule {
    state: web::Data<AppState>,
}

#[derive(Clone)]
pub struct Permissions {
    pub create_product: Permission,
}

impl CatalogModule {
    async fn default_pool() -> Result<Pool<Sqlite>, Error> {
        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://catalog.db?mode=rwc".to_string());

        Ok(SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?)
    }

    async fn default_repo() -> Result<ProductRepository, Error> {
        Ok(ProductRepository::new(CatalogModule::default_pool().await?).await?)
    }

    /// Construct catalog web module using default values.
    /// The default values are: db and on_create handler
    pub async fn default(perms: Vec<Permission>) -> Result<Self, Error> {
        let repo = CatalogModule::default_repo().await?;
        let state = AppState {
            tera: Tera::new("templates/**/*").unwrap(),
            caches: Caches::new(),
            service: Service::new(repo.clone()),
            repo,
            permissions: CatalogModule::set_permissions(perms),
            on_create_product: Box::new(DefaultOnCreate {}),
        };
        Ok(Self {
            state: web::Data::new(state),
        })
    }

    pub fn get_permissions() -> Vec<String> {
        vec!["create_product".to_string()]
    }

    pub fn set_permissions(mut perms: Vec<Permission>) -> Permissions {
        Permissions {
            create_product: perms.pop().unwrap(),
        }
    }

    pub async fn new(cfg: Config) -> Result<CatalogModule, Error> {
        let repo = cfg.repo.unwrap_or(CatalogModule::default_repo().await?);
        let state = AppState {
            tera: Tera::new("templates/**/*").unwrap(),
            service: Service::new(repo.clone()),
            caches: Caches::new(),
            repo,
            permissions: cfg.permissions.unwrap(),
            on_create_product: cfg
                .on_create_product
                .unwrap_or(Box::new(DefaultOnCreate {})),
        };
        Ok(Self {
            state: web::Data::new(state),
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
                        .wrap(Auth)
                        .service(routes::create_product),
                ),
        );
    }
}
