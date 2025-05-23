use actix_web::{web, HttpResponse};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;
use crate::middleware::auth::Claims;
use crate::db::health_data::insert_health_data;
use crate::models::health_data::{HealthDataSyncRequest, HealthDataSyncResponse};
use redis::AsyncCommands;

#[tracing::instrument(
    name = "Upload health data",
    skip(data, pool, redis, claims),
    fields(
        username = %claims.username,
        data_type = %data.device_id
    )
)]

pub async fn upload_health_data(
    data: web::Json<HealthDataSyncRequest>,
    pool: web::Data<sqlx::PgPool>,
    redis: Option<web::Data<redis::Client>>,
    claims: web::ReqData<Claims>
) -> HttpResponse {
    tracing::info!("Sync health data handler called from device: {}", data.device_id);
    let user_id = match Uuid::parse_str(&claims.sub) {
        Ok(id) => {
            tracing::info!("User ID parsed successfully: {}", id);
            id
        },
        Err(e) => {
            tracing::error!("Failed to parse user ID: {}", e);
            return HttpResponse::InternalServerError().json(json!({
                "status": "error",
                "message": "Invalid user ID"
            }));
        }
    };    
    // Insert health data into database
    let insert_result = insert_health_data(&pool, user_id, &data).await;
    
    match insert_result {
        Ok(sync_id) => {
            // Identify the type of data for event classification
            let data_types = determine_data_types(&data);
            tracing::info!("Data types detected: {:?}", data_types);

            let event = serde_json::json!({
                "event_type": "health_data_uploaded",
                "user_id": user_id.to_string(),
                "sync_id": sync_id.to_string(),
                "timestamp": Utc::now().to_rfc3339(),
                "data_types": data_types
            });

            let message_str = serde_json::to_string(&event)
                .unwrap_or_else(|e| {
                    tracing::error!("Failed to serialize Redis message: {}", e);
                    "{}".to_string()
                });

            if let Some(redis_client) = &redis {
                match redis_client.get_async_connection().await {
                    Ok(mut conn) => {
                        let pub_result: Result<i32, redis::RedisError> = conn.publish("evolveme:events:health_data", message_str).await;
                        match pub_result {
                            Ok(receivers) => {
                                tracing::info!("Successfully published health data event for sync_id: {} to {} receivers",
                                    sync_id, receivers);
                            }
                            Err(e) => {
                                tracing::error!("Failed to publish health data event: {}", e);
                            }
                        }
                    },
                    Err(e) => {
                        tracing::error!("Failed to get Redis connection: {}", e);
                    }
                }
            }

            // Prepare successful response
            let response = HealthDataSyncResponse {
                success: true,
                message: "Health data synced successfully".to_string(),
                sync_id,
                timestamp: Utc::now(),
            };
            tracing::info!("Health data synced successfully: {}", sync_id); 
            HttpResponse::Ok().json(response)
        }
        Err(e) => {
            // Prepare error response
            let response = HealthDataSyncResponse {
                success: false,
                message: format!("Failed to sync health data: {}", e),
                sync_id: uuid::Uuid::nil(), // Use nil UUID for error case
                timestamp: Utc::now(),
            };
            tracing::error!("Failed to sync health data: {}", e);
            HttpResponse::InternalServerError().json(response)
        }
    }
}

fn determine_data_types(data: &HealthDataSyncRequest) -> Vec<String> {
    let mut data_types = Vec::new();
    
    if data.sleep.is_some() {
        data_types.push("sleep".to_string());
    }
    
    if data.steps.is_some() && data.steps.unwrap() > 0 {
        data_types.push("steps".to_string());
    }
    
    if data.heart_rate.is_some() {
        data_types.push("heart_rate".to_string());
    }
    
    if data.active_energy_burned.is_some() {
        data_types.push("energy".to_string());
    }
    
    if data.additional_metrics.is_some() {
        // Extract metrics from additional_metrics
        if let Some(ref metrics) = data.additional_metrics {
            let json_value = serde_json::to_value(metrics).unwrap_or(serde_json::Value::Null);
            
            if let serde_json::Value::Object(obj) = json_value {
                // Check for specific additional metrics we care about
                if obj.contains_key("blood_oxygen") {
                    data_types.push("blood_oxygen".to_string());
                }
                
                if obj.contains_key("respiratory_rate") {
                    data_types.push("respiratory".to_string());
                }
                
                if obj.contains_key("hrv") {
                    data_types.push("hrv".to_string());
                }
                
                if obj.contains_key("stress_level") {
                    data_types.push("stress".to_string());
                }
            }
        }
    }
    
    // If no specific types are detected, add a generic "health" type
    if data_types.is_empty() {
        data_types.push("health".to_string());
    }
    
    data_types
}