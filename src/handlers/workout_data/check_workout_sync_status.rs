use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::auth::Claims;
use crate::models::common::ApiResponse;

#[derive(Debug, Deserialize)]
pub struct CheckSyncStatusRequest {
    pub workout_uuids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SyncStatusResponse {
    pub synced_workouts: Vec<String>,
    pub unsynced_workouts: Vec<String>,
}

#[tracing::instrument(
    name = "Check workout sync status",
    skip(pool, claims, request),
    fields(
        username = %claims.username,
        workout_uuids = %request.workout_uuids.join(", ")
    )
)]
pub async fn check_workout_sync_status(
    pool: web::Data<PgPool>,
    claims: web::ReqData<Claims>,
    request: web::Json<CheckSyncStatusRequest>,
) -> HttpResponse {
    tracing::info!("🎮 Checking workout sync status for user: {}", claims.username);

    let user_id = match Uuid::parse_str(&claims.sub) {
        Ok(id) => {
            tracing::info!("User ID parsed successfully: {}", id);
            id
        },
        Err(e) => {
            tracing::error!("Failed to parse user ID: {}", e);
            return HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error("Invalid user ID")
            );
        }
    };
    let workout_uuids = &request.workout_uuids;

    // Query to find which workout UUIDs already exist for this user
    let existing_uuids = match sqlx::query_scalar!(
        r#"
        SELECT workout_uuid 
        FROM workout_data 
        WHERE user_id = $1 
        AND workout_uuid = ANY($2)
        "#,
        user_id,
        workout_uuids
    )
    .fetch_all(pool.get_ref())
    .await {
        Ok(uuids) => uuids,
        Err(e) => {
            tracing::error!("Database error checking workout sync status: {:?}", e);
            return HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error("Failed to check sync status")
            );
        }
    };

    // Since workout_uuid is now NOT NULL, we can directly use the results
    let synced_workouts: Vec<String> = existing_uuids;
    
    let unsynced_workouts: Vec<String> = workout_uuids
        .iter()
        .filter(|uuid| !synced_workouts.contains(uuid))
        .cloned()
        .collect();

    let response = SyncStatusResponse {
        synced_workouts,
        unsynced_workouts,
    };

    tracing::info!("✅ Sync status check completed successfully");
    HttpResponse::Ok().json(ApiResponse::success(
        "Sync status retrieved successfully",
        response,
    ))
}