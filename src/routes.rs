use crate::models::{AppState, CreateProductDto, SafeProductQuery, UpdateProductDto};
use actix_web::{HttpResponse, Responder, get, post, web};
use ferrumec::crypto::Claims;
use ferrumec::CreateItem;
use serde_json::to_string;
use tera::Context;

#[post("/products")]
pub async fn create_product(
    state: web::Data<AppState>,
    claims: web::ReqData<Claims>,
    payload: web::Json<CreateProductDto>,
) -> impl Responder {
    if !state.permissions.create_product.check(claims.into_inner()) {
        return HttpResponse::Forbidden().body("Unauthorized");
    }
    let dto = payload.into_inner();
    match state.repo.create(dto.clone()).await {
        Ok(product) => {
            let prod = product.clone();
            state.es.publish(
                "product-created".to_string(),
                to_string(&CreateItem {
                    name: product.name,
                    id: product.id,
                    sku: product.sku,
                    quantity: dto.qty,
                })
                .unwrap()
                .into_bytes(),
            );
            HttpResponse::Created().json(prod)
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
    query: web::Query<SafeProductQuery>,
) -> impl Responder {
    match data.service.list_products(query.into_inner().into()).await {
        Ok(products) => HttpResponse::Ok().json(products),
        Err(err) => {
            eprintln!("Error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("/")]
pub async fn index(
    data: web::Data<AppState>,
    query: web::Query<SafeProductQuery>,
) -> impl Responder {
    if let Some(cached_htnl) = data.caches.catalog_page.get("catalog").await {
        return HttpResponse::Ok()
            .content_type("text/html")
            .body(cached_htnl);
    }
    let products = match data
        .service
        .list_products(query.clone().into_inner().into())
        .await
    {
        Ok(r) => r,
        Err(_e) => return HttpResponse::InternalServerError().finish(),
    };
    let mut ctx = Context::new();
    let categories = match data.service.get_categories().await {
        Ok(r) => r,
        Err(_e) => return HttpResponse::InternalServerError().finish(),
    };
    ctx.insert("products", &products);
    ctx.insert("categories", &categories);
    ctx.insert(
        "category",
        &query.category.clone().unwrap_or("==".to_string()),
    );
    ctx.insert("sort", "desc");
    ctx.insert("total_pages", &1);
    let rendered_html = data.tera.render("catalog.html", &ctx).unwrap();
    data.caches
        .catalog_page
        .insert("catalog".to_string(), rendered_html.clone())
        .await;
    HttpResponse::Ok().body(rendered_html)
}

#[post("/products/{id}")]
pub async fn update(
    data: web::Data<AppState>,
    product: web::Path<String>,
    query: web::Query<UpdateProductDto>,
) -> impl Responder {
    match data
        .repo
        .update(product.into_inner(), query.into_inner())
        .await
    {
        Ok(_) => HttpResponse::Ok(),
        Err(e) => {
            eprintln!("Error in updating product: {}", e);
            HttpResponse::InternalServerError()
        }
    }
}
