//! Small HTTP adapters. Work runs in the background; polling reads durable snapshots.
use crate::{
    crawler::Engine,
    llm,
    models::{RunOptions, Settings},
};
use actix_web::{HttpRequest, HttpResponse, web};
use serde_json::json;
use std::sync::Arc;
type State = web::Data<Arc<Engine>>;
fn failure(status: actix_web::http::StatusCode, message: impl ToString) -> HttpResponse {
    HttpResponse::build(status).json(json!({"error":message.to_string()}))
}
fn internal(e: impl ToString) -> HttpResponse {
    failure(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, e)
}
fn mutation_allowed(req: &HttpRequest) -> bool {
    req.headers()
        .get("x-crawler-request")
        .is_some_and(|v| v == "1")
        && req.headers().get("origin").is_none_or(|v| {
            crate::server_port().is_ok_and(|port| {
                v == format!("http://127.0.0.1:{port}").as_str()
                    || v == format!("http://localhost:{port}").as_str()
            })
        })
}
async fn get_settings(state: State) -> HttpResponse {
    match state.store.settings() {
        Ok(s) => HttpResponse::Ok().json(s),
        Err(e) => internal(e),
    }
}
async fn save_settings(req: HttpRequest, state: State, body: web::Json<Settings>) -> HttpResponse {
    if !mutation_allowed(&req) {
        return failure(
            actix_web::http::StatusCode::FORBIDDEN,
            "請從本機管理介面操作",
        );
    }
    if let Err(e) = body.validate() {
        return failure(actix_web::http::StatusCode::BAD_REQUEST, e);
    }
    match llm::models().await {
        Ok(models) if models.contains(&body.model) => {}
        Ok(_) => return failure(actix_web::http::StatusCode::BAD_REQUEST, "模型未安裝"),
        Err(e) => return failure(actix_web::http::StatusCode::SERVICE_UNAVAILABLE, e),
    }
    match state.store.save_settings(&body) {
        Ok(()) => HttpResponse::Ok().json(body.into_inner()),
        Err(e) => internal(e),
    }
}
async fn models() -> HttpResponse {
    match llm::models().await {
        Ok(m) => HttpResponse::Ok().json(json!({"models":m})),
        Err(e) => failure(actix_web::http::StatusCode::SERVICE_UNAVAILABLE, e),
    }
}
async fn start(req: HttpRequest, state: State, options: web::Json<RunOptions>) -> HttpResponse {
    if !mutation_allowed(&req) {
        return failure(
            actix_web::http::StatusCode::FORBIDDEN,
            "請從本機管理介面操作",
        );
    }
    match state.start(options.into_inner()) {
        Ok(job) => HttpResponse::Accepted().json(job),
        Err(e) => failure(actix_web::http::StatusCode::CONFLICT, e),
    }
}
async fn jobs(state: State) -> HttpResponse {
    match state.store.jobs(){Ok(jobs)=>HttpResponse::Ok().json(json!({"active_id":state.active_id(),"jobs":jobs.into_iter().map(|j|json!({"id":j.id,"status":j.status,"phase":j.phase,"started_at":j.started_at,"finished_at":j.finished_at,"discovered":j.discovered,"processed":j.processed,"succeeded":j.succeeded,"failed":j.failed,"pages":j.pages.len(),"issue_count":j.issues.len()})).collect::<Vec<_>>()})),Err(e)=>internal(e)}
}
async fn job(state: State, id: web::Path<String>) -> HttpResponse {
    match state.store.job(&id) {
        Ok(Some(job)) => HttpResponse::Ok().json(job),
        Ok(None) => failure(actix_web::http::StatusCode::NOT_FOUND, "找不到工作"),
        Err(e) => internal(e),
    }
}
async fn cancel(req: HttpRequest, state: State, id: web::Path<String>) -> HttpResponse {
    if !mutation_allowed(&req) {
        return failure(
            actix_web::http::StatusCode::FORBIDDEN,
            "請從本機管理介面操作",
        );
    }
    match state.cancel(&id) {
        Ok(()) => HttpResponse::Accepted().json(json!({"status":"cancelling"})),
        Err(e) => failure(actix_web::http::StatusCode::CONFLICT, e),
    }
}
async fn download(state: State, id: web::Path<String>) -> HttpResponse {
    match state.store.job(&id) {
        Ok(Some(job)) => HttpResponse::Ok()
            .insert_header((
                "Content-Disposition",
                format!("attachment; filename=\"apple-tw-{}.json\"", job.id),
            ))
            .json(job),
        Ok(None) => failure(actix_web::http::StatusCode::NOT_FOUND, "找不到工作"),
        Err(e) => internal(e),
    }
}
async fn script() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(include_str!("app.js"))
}
pub fn routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/app.js", web::get().to(script)).service(
        web::scope("/api")
            .app_data(web::JsonConfig::default().limit(16384))
            .route("/settings", web::get().to(get_settings))
            .route("/settings", web::post().to(save_settings))
            .route("/models", web::get().to(models))
            .route("/jobs", web::get().to(jobs))
            .route("/jobs", web::post().to(start))
            .route("/jobs/{id}", web::get().to(job))
            .route("/jobs/{id}/cancel", web::post().to(cancel))
            .route("/jobs/{id}/download", web::get().to(download)),
    );
}
