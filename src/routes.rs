use crate::models::{AppState, CreateProductDto, ProductQuery};
use actix_web::{HttpResponse, Responder, get, post, web};
use auth_middleware::Claims;

#[post("/products")]
pub async fn create_product(
    state: web::Data<AppState>,
    claims: web::ReqData<Claims>,
    payload: web::Json<CreateProductDto>,
) -> impl Responder {
    if !state.permissions.create_product.check(claims.into_inner()) {
        return HttpResponse::Unauthorized().finish();
    }
    let dto = payload.into_inner();
    match state.repo.create(dto.clone()).await {
        Ok(product) => {
            state.on_create_product.handle(dto).await;
            HttpResponse::Created().json(product)
        }
        Err(err) => {
            eprintln!("Error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("/products/{id}")]
pub async fn get_product(data: web::Data<AppState>, path: web::Path<String>) -> impl Responder {
    match data.repo.find_by_id(path.into_inner()).await {
        Ok(Some(product)) => HttpResponse::Ok().json(product),
        Ok(None) => {
            HttpResponse::NotFound().json(serde_json::json!({ "error": "Product not found" }))
        }
        Err(err) => {
            eprintln!("DB error: {err}");
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("/products/slug/{slug}")]
pub async fn get_product_by_slug(
    data: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let slug = path.into_inner();
    match data.repo.find_by_slug(&slug).await {
        Ok(Some(product)) => HttpResponse::Ok().json(product),
        Ok(None) => {
            HttpResponse::NotFound().json(serde_json::json!({ "error": "Product not found" }))
        }
        Err(err) => {
            eprintln!("DB error: {err}");
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("/products")]
pub async fn list_products(
    data: web::Data<AppState>,
    query: web::Query<ProductQuery>,
) -> impl Responder {
    match data.repo.find_all(query.into_inner()).await {
        Ok(products) => HttpResponse::Ok().json(products),
        Err(err) => {
            eprintln!("Error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}
