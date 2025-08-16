use actix_web::{post, get, put, web, HttpResponse};
use crate::handlers::workout_data::upload_workout_data::upload_workout_data;
use crate::handlers::workout_data::media_upload::{upload_workout_media, serve_workout_media};
use crate::handlers::workout_data::update_workout_media::update_workout_media;
use crate::middleware::auth::Claims;
use crate::models::workout_data::WorkoutDataSyncRequest;
use crate::services::live_game_service::LiveGameService;
use std::sync::Arc;

#[post("/upload_health")]
async fn upload_health(
    data: web::Json<WorkoutDataSyncRequest>,
    pool: web::Data<sqlx::PgPool>,
    redis: Option<web::Data<Arc<redis::Client>>>,
    live_game_service: Option<web::Data<LiveGameService>>,
    claims: web::ReqData<Claims>
) -> HttpResponse {
    upload_workout_data(data, pool, redis, live_game_service, claims).await
}

#[post("/upload_workout_media")]
async fn upload_media(
    form: actix_multipart::form::MultipartForm<crate::handlers::workout_data::media_upload::MediaUploadForm>,
    claims: web::ReqData<Claims>
) -> HttpResponse {
    upload_workout_media(form, claims).await
}

#[get("/workout-media/{filename}")]
async fn serve_media(
    path: web::Path<String>
) -> HttpResponse {
    serve_workout_media(path).await
}

#[put("/workout/{workout_id}/media")]
async fn update_media(
    workout_id: web::Path<String>,
    data: web::Json<crate::handlers::workout_data::update_workout_media::UpdateWorkoutMediaRequest>,
    pool: web::Data<sqlx::PgPool>,
    claims: web::ReqData<Claims>
) -> HttpResponse {
    // Parse workout_id from path and merge with request data
    let workout_uuid = match uuid::Uuid::parse_str(&workout_id.into_inner()) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(
                crate::models::common::ApiResponse::<()>::error("Invalid workout ID")
            );
        }
    };
    
    let mut request_data = data.into_inner();
    request_data.workout_id = workout_uuid;
    
    update_workout_media(web::Json(request_data), pool, claims).await
}