use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::auth::Claims;
use crate::models::common::ApiResponse;

#[derive(Debug, Deserialize)]
pub struct UpdateWorkoutMediaRequest {
    pub workout_id: Uuid,
    pub image_url: Option<String>,
    pub video_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateWorkoutMediaResponse {
    pub workout_id: Uuid,
    pub image_url: Option<String>,
    pub video_url: Option<String>,
    pub updated: bool,
}

#[tracing::instrument(
    name = "Update workout media URLs",
    skip(data, pool, claims),
    fields(
        username = %claims.username,
        workout_id = %data.workout_id
    )
)]
pub async fn update_workout_media(
    data: web::Json<UpdateWorkoutMediaRequest>,
    pool: web::Data<sqlx::PgPool>,
    claims: web::ReqData<Claims>,
) -> HttpResponse {
    tracing::info!("📎 Updating media for workout: {}", data.workout_id);

    let user_id = match Uuid::parse_str(&claims.sub) {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Failed to parse user ID: {}", e);
            return HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error("Invalid user ID")
            );
        }
    };

    // First verify the workout belongs to the user
    let workout_check = sqlx::query!(
        r#"
        SELECT id FROM workout_data 
        WHERE id = $1 AND user_id = $2
        "#,
        data.workout_id,
        user_id
    )
    .fetch_optional(&**pool)
    .await;

    match workout_check {
        Ok(Some(_)) => {
            // Workout exists and belongs to user, proceed with update
            tracing::info!("✅ Workout verified for user {}", claims.username);
        }
        Ok(None) => {
            tracing::warn!("❌ Workout {} not found or doesn't belong to user {}", 
                data.workout_id, claims.username);
            return HttpResponse::NotFound().json(
                ApiResponse::<()>::error("Workout not found or access denied")
            );
        }
        Err(e) => {
            tracing::error!("Database error checking workout: {}", e);
            return HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error("Failed to verify workout")
            );
        }
    }

    // Update the workout with media URLs
    let update_result = sqlx::query!(
        r#"
        UPDATE workout_data 
        SET 
            image_url = COALESCE($1, image_url),
            video_url = COALESCE($2, video_url),
            updated_at = NOW()
        WHERE id = $3 AND user_id = $4
        RETURNING id, image_url, video_url
        "#,
        data.image_url.as_deref(),
        data.video_url.as_deref(),
        data.workout_id,
        user_id
    )
    .fetch_optional(&**pool)
    .await;

    match update_result {
        Ok(Some(record)) => {
            tracing::info!("✅ Successfully updated media for workout {}", data.workout_id);
            
            let response = UpdateWorkoutMediaResponse {
                workout_id: record.id,
                image_url: record.image_url,
                video_url: record.video_url,
                updated: true,
            };

            HttpResponse::Ok().json(
                ApiResponse::success("Workout media updated successfully", response)
            )
        }
        Ok(None) => {
            tracing::error!("Failed to update workout media - no rows returned");
            HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error("Failed to update workout media")
            )
        }
        Err(e) => {
            tracing::error!("Database error updating workout media: {}", e);
            HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error("Failed to update workout media")
            )
        }
    }
}