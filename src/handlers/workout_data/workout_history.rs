use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use sqlx::PgPool;
use chrono::{DateTime, Utc, Duration};

use crate::{middleware::auth::Claims, models::workout_data::HeartRateData};

#[derive(Debug, Serialize)]
pub struct WorkoutHistoryItem {
    pub id: Uuid,
    pub workout_date: DateTime<Utc>,
    pub workout_start: Option<DateTime<Utc>>,
    pub workout_end: Option<DateTime<Utc>>,
    pub duration_minutes: Option<i32>,
    pub calories_burned: Option<i32>,
    pub avg_heart_rate: Option<i32>,
    pub max_heart_rate: Option<i32>,
    pub heart_rate_zones: Option<serde_json::Value>,
    // Game stats gained from this workout
    pub stamina_gained: i32,
    pub strength_gained: i32,
    // Media attachments
    pub image_url: Option<String>,
    pub video_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WorkoutHistoryQuery {
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

fn calculate_duration_minutes(start: Option<DateTime<Utc>>, end: Option<DateTime<Utc>>) -> Option<i32> {
    match (start, end) {
        (Some(s), Some(e)) => {
            let duration = e.signed_duration_since(s);
            // Only return positive durations
            if duration > Duration::zero() {
                Some((duration.num_seconds() / 60) as i32)
            } else {
                None
            }
        }
        _ => None
    }
}

fn calculate_avg_heart_rate(heart_rate_data: &Vec<HeartRateData>) -> Option<i32> {
    if heart_rate_data.is_empty() {
        return None;
    }
    let sum: i32 = heart_rate_data.iter().map(|hr| hr.heart_rate).sum();
    let count = heart_rate_data.len() as i32;
    Some((sum / count) as i32)
}

fn calculate_max_heart_rate(heart_rate_data: &Vec<HeartRateData>) -> Option<i32> {
    heart_rate_data.iter().map(|hr| hr.heart_rate).reduce(i32::max)
}

#[tracing::instrument(
    name = "Get user workout history",
    skip(pool, claims),
    fields(username = %claims.username)
)]
pub async fn get_workout_history(
    pool: web::Data<PgPool>,
    claims: web::ReqData<Claims>,
    query: web::Query<WorkoutHistoryQuery>,
) -> HttpResponse {
    let user_id = match Uuid::parse_str(&claims.sub) {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Failed to parse user ID: {}", e);
            return HttpResponse::BadRequest().json(json!({
                "error": "Invalid user ID"
            }));
        }
    };

    let limit = query.limit.unwrap_or(20).min(100); // Max 100 items
    let offset = query.offset.unwrap_or(0);

    // Fetch workout history with all stats from workout_data
    let workouts: Vec<WorkoutHistoryItem> = match sqlx::query!(
        r#"
        SELECT 
            wd.id,
            COALESCE(wd.workout_start, wd.created_at) as workout_date,
            wd.workout_start,
            wd.workout_end,
            wd.created_at,
            wd.calories_burned as calories_burned,
            wd.duration_minutes,
            wd.avg_heart_rate,
            wd.max_heart_rate,
            wd.heart_rate_data,
            wd.heart_rate_zones,
            COALESCE(wd.stamina_gained, 0) as stamina_gained,
            COALESCE(wd.strength_gained, 0) as strength_gained,
            wd.image_url,
            wd.video_url
        FROM workout_data wd
        WHERE wd.user_id = $1
        AND (wd.calories_burned > 100 OR wd.heart_rate_data IS NOT NULL)
        ORDER BY COALESCE(wd.workout_start, wd.created_at) DESC
        LIMIT $2 OFFSET $3
        "#,
        user_id,
        limit as i64,
        offset as i64
    )
    .fetch_all(&**pool)
    .await
    {
        Ok(rows) => {
            rows.into_iter().map(|row| {
                // Use pre-calculated values from database when available
                let duration_minutes = row.duration_minutes
                    .or_else(|| calculate_duration_minutes(row.workout_start, row.workout_end));
                
                // Use pre-calculated heart rate stats or calculate from raw data if needed
                let (avg_heart_rate, max_heart_rate) = if row.avg_heart_rate.is_some() && row.max_heart_rate.is_some() {
                    (row.avg_heart_rate, row.max_heart_rate)
                } else {
                    // Fallback: calculate from raw data if pre-calculated values missing
                    let heart_rate_data: Vec<HeartRateData> = 
                        serde_json::from_value(row.heart_rate_data.clone()).unwrap_or_default();
                    (
                        calculate_avg_heart_rate(&heart_rate_data),
                        calculate_max_heart_rate(&heart_rate_data)
                    )
                };
                
                WorkoutHistoryItem {
                    id: row.id,
                    workout_date: row.workout_date.unwrap_or(row.created_at),
                    workout_start: row.workout_start,
                    workout_end: row.workout_end,
                    duration_minutes,
                    calories_burned: row.calories_burned,
                    avg_heart_rate,
                    max_heart_rate,
                    heart_rate_zones: row.heart_rate_zones, // Now directly from workout_data
                    stamina_gained: row.stamina_gained.unwrap_or(0) as i32,
                    strength_gained: row.strength_gained.unwrap_or(0) as i32,
                    image_url: row.image_url,
                    video_url: row.video_url,
                }
            }).collect()
        },
        Err(e) => {
            tracing::error!("Failed to fetch workout history: {}", e);
            return HttpResponse::InternalServerError().json(json!({
                "error": "Failed to fetch workout history"
            }));
        }
    };

    // Get total count for pagination
    let total_count = match sqlx::query!(
        r#"
        SELECT COUNT(*) as count
        FROM workout_data
        WHERE user_id = $1
        AND (calories_burned > 100 OR heart_rate_data IS NOT NULL)
        "#,
        user_id
    )
    .fetch_one(&**pool)
    .await
    {
        Ok(row) => row.count.unwrap_or(0),
        Err(_) => 0,
    };
    

    tracing::info!(
        "Successfully retrieved {} workouts for user: {}",
        workouts.len(),
        claims.username
    );

    HttpResponse::Ok().json(json!({
        "success": true,
        "data": {
            "workouts": workouts,
            "pagination": {
                "total": total_count,
                "limit": limit,
                "offset": offset,
                "has_more": (offset as i64 + limit as i64) < total_count
            }
        }
    }))
}