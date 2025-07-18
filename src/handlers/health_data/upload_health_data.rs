// Enhanced src/handlers/health_data/upload_health_data.rs - Now with game stats!

use actix_web::{web, HttpResponse};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;
use crate::middleware::auth::Claims;
use crate::db::health_data::{insert_health_data, check_workout_uuid_exists};
use crate::models::health_data::HealthDataSyncRequest;
use crate::models::common::ApiResponse;
use crate::game::stats_calculator::StatCalculator;
use crate::models::game_events::GameEvent;
use crate::models::live_game::LiveGameScoreUpdate;
use crate::services::live_game_service::LiveGameService;
use redis::AsyncCommands;

#[tracing::instrument(
    name = "Upload health data with game stats",
    skip(data, pool, redis, live_game_service, claims),
    fields(
        username = %claims.username,
        data_type = %data.device_id
    )
)]
pub async fn upload_health_data(
    data: web::Json<HealthDataSyncRequest>,
    pool: web::Data<sqlx::PgPool>,
    redis: Option<web::Data<redis::Client>>,
    live_game_service: Option<web::Data<LiveGameService>>,
    claims: web::ReqData<Claims>
) -> HttpResponse {
    tracing::info!("🎮 Processing health data with game mechanics for user: {}", claims.username);
    
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

    // 🔍 CHECK FOR DUPLICATE WORKOUT
    if let Some(workout_uuid) = &data.workout_uuid {
        tracing::info!("🔍 Checking for duplicate workout UUID: {}", workout_uuid);
        
        match check_workout_uuid_exists(&pool, user_id, workout_uuid).await {
            Ok(exists) => {
                if exists {
                    tracing::info!("⚠️ Duplicate workout detected for {}: {}", claims.username, workout_uuid);
                    return HttpResponse::Ok().json(
                        ApiResponse::success(
                            "Workout already processed - skipping duplicate", 
                            json!({
                                "duplicate": true,
                                "workout_uuid": workout_uuid,
                                "message": "This workout has already been processed"
                            })
                        )
                    );
                }
            }
            Err(e) => {
                tracing::error!("❌ Failed to check for duplicate workout: {}", e);
                return HttpResponse::InternalServerError().json(
                    ApiResponse::<()>::error("Failed to check for duplicate workout")
                );
            }
        }
    }

    // 🎲 CALCULATE GAME STATS FROM HEALTH DATA
    let stat_changes = StatCalculator::calculate_stat_changes(&pool, user_id, &data).await;
    tracing::info!("📊 Calculated stat changes for {}: +{} stamina, +{} strength", 
        claims.username, 
        stat_changes.stamina_change, 
        stat_changes.strength_change, 
    );

    // 💾 APPLY STAT CHANGES TO DATABASE
    let update_result = sqlx::query!(
        r#"
        UPDATE user_avatars 
        SET stamina = stamina + $1, 
            strength = strength + $2
        WHERE user_id = $3
        "#,
        stat_changes.stamina_change,
        stat_changes.strength_change,
        user_id
    )
    .execute(&**pool)
    .await;

    match update_result {
        Ok(_) => {
            tracing::info!("✅ Successfully updated avatar stats for {}", claims.username);
        }
        Err(e) => {
            tracing::error!("❌ Failed to update avatar stats for {}: {}", claims.username, e);
            return HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error("Failed to update avatar stats")
            );
        }
    }

    // 🏆 CHECK FOR ACTIVE LIVE GAMES AND UPDATE SCORES
    if let Some(live_service) = &live_game_service {
        check_and_update_live_games(
            user_id, 
            &claims.username,
            &stat_changes,
            live_service,
            &pool
        ).await;
    }

    // Insert health data into database
    let insert_result = insert_health_data(&pool, user_id, &data).await;
    
    match insert_result {
        Ok(sync_id) => {
            // 🎯 PREPARE GAME EVENT FOR REAL-TIME NOTIFICATION
            let game_event = json!({
                "event_type": "health_data_processed",
                "user_id": user_id.to_string(),
                "username": claims.username,
                "sync_id": sync_id.to_string(),
                "stat_changes": {
                    "stamina_change": stat_changes.stamina_change,
                    "strength_change": stat_changes.strength_change,
                },
                "reasoning": stat_changes.reasoning,
                "timestamp": Utc::now().to_rfc3339()
            });

            // 📡 PUBLISH TO REDIS FOR REAL-TIME NOTIFICATION
            if let Some(redis_client) = &redis {
                let user_channel = format!("game:events:user:{}", user_id);
                let global_channel = "game:events:global".to_string();
                let event_str = serde_json::to_string(&game_event)
                    .unwrap_or_else(|e| {
                        tracing::error!("Failed to serialize game event: {}", e);
                        "{}".to_string()
                    });

                let redis_client = redis_client.clone();
                let event_str_clone = event_str.clone();
                let username_clone = claims.username.clone();
                
                tokio::spawn(async move {
                    match redis_client.get_async_connection().await {
                        Ok(mut conn) => {
                            // Publish to user-specific channel
                            let user_result: Result<i32, redis::RedisError> = 
                                conn.publish(&user_channel, &event_str).await;
                            
                            // Also publish to global channel for leaderboards/social features
                            let global_result: Result<i32, redis::RedisError> = 
                                conn.publish(&global_channel, &event_str_clone).await;
                            
                            match (user_result, global_result) {
                                (Ok(user_receivers), Ok(global_receivers)) => {
                                    tracing::info!("🎮 Published game event for {} to {} user subscribers and {} global subscribers", 
                                        username_clone, user_receivers, global_receivers);
                                }
                                (Err(e), _) | (_, Err(e)) => {
                                    tracing::error!("❌ Failed to publish game event for {}: {}", username_clone, e);
                                }
                            }
                        },
                        Err(e) => {
                            tracing::error!("❌ Redis connection failed during game event publishing: {}", e);
                        }
                    }
                });
            } else {
                tracing::warn!("⚠️  Redis not available - game events will not be published in real-time");
            }

            // 🎉 ENHANCED RESPONSE WITH GAME STATS
            let sync_data = json!({
                "sync_id": sync_id,
                "timestamp": Utc::now(),
                "game_stats": {
                    "stat_changes": {
                        "stamina_change": stat_changes.stamina_change,
                        "strength_change": stat_changes.strength_change,
                    },
                    "reasoning": stat_changes.reasoning,
                    "summary": format!("Gained {} total stat points!", 
                        stat_changes.stamina_change + stat_changes.strength_change
                    )
                }
            });

            tracing::info!("✅ Health data processed successfully with game mechanics for {}: {}", 
                claims.username, sync_id);
            HttpResponse::Ok().json(
                ApiResponse::success("Health data synced and game stats calculated!", sync_data)
            )
        }
        Err(e) => {
            tracing::error!("❌ Failed to sync health data for {}: {}", claims.username, e);
            HttpResponse::InternalServerError().json(
                ApiResponse::<()>::error(format!("Failed to sync health data: {}", e))
            )
        }
    }
}

/// Check if user is in any active live games and update scores
async fn check_and_update_live_games(
    user_id: Uuid,
    username: &str,
    stat_changes: &crate::game::stats_calculator::StatChanges,
    live_game_service: &LiveGameService,
    pool: &sqlx::PgPool,
) {
    tracing::info!("🎮 Checking for active live games for user {}", username);

    // Get user's active live games
    match live_game_service.get_user_active_games(user_id).await {
        Ok(active_games) => {
            if active_games.is_empty() {
                tracing::debug!("No active live games found for user {}", username);
                return;
            }

            tracing::info!("🏆 Found {} active live game(s) for user {}", active_games.len(), username);

            for live_game in active_games {
                // Determine which team the user belongs to
                let user_team_id = if let Ok(team_id) = get_user_team_id(user_id, &live_game, pool).await {
                    team_id
                } else {
                    tracing::error!("Could not determine team for user {} in game {}", username, live_game.game_id);
                    continue;
                };

                // Calculate score increases based on stat changes
                let score_increase = live_game_service.calculate_score_from_stats(
                    stat_changes.stamina_change,
                    stat_changes.strength_change,
                );
                let power_increase = live_game_service.calculate_power_from_stats(
                    stat_changes.stamina_change,
                    stat_changes.strength_change,
                );
                
                tracing::info!("📊 Score calculation for {}: stamina={}, strength={}, score_increase={}, power_increase={}, team_id={}", 
                    username, stat_changes.stamina_change, stat_changes.strength_change, 
                    score_increase, power_increase, user_team_id);

                // Create the score update
                let score_update = LiveGameScoreUpdate {
                    user_id,
                    username: username.to_string(),
                    team_id: user_team_id,
                    score_increase,
                    power_increase,
                    stamina_gained: stat_changes.stamina_change,
                    strength_gained: stat_changes.strength_change,
                    description: format!("Workout upload: +{} stamina, +{} strength", 
                        stat_changes.stamina_change, stat_changes.strength_change),
                };

                // Apply the score update
                match live_game_service.handle_score_update(live_game.game_id, score_update).await {
                    Ok(updated_game) => {
                        tracing::info!("📊 Updated live game {}: {} {} - {} {} (Player: {} +{})", 
                            live_game.game_id,
                            updated_game.home_team_name,
                            updated_game.home_score,
                            updated_game.away_score,
                            updated_game.away_team_name,
                            username,
                            score_increase
                        );
                    }
                    Err(e) => {
                        tracing::error!("❌ Failed to update live game score for {}: {}", username, e);
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("❌ Failed to get active games for user {}: {}", username, e);
        }
    }
}

/// Helper function to determine which team a user belongs to in a live game
async fn get_user_team_id(
    user_id: Uuid, 
    live_game: &crate::models::live_game::LiveGame,
    pool: &sqlx::PgPool
) -> Result<Uuid, Box<dyn std::error::Error>> {
    // Query live_player_contributions to find which team the user belongs to
    let team_info = sqlx::query!(
        r#"
        SELECT team_id, team_side 
        FROM live_player_contributions 
        WHERE live_game_id = $1 AND user_id = $2
        "#,
        live_game.id,
        user_id
    )
    .fetch_optional(pool)
    .await?;
    
    match team_info {
        Some(info) => Ok(info.team_id),
        None => {
            // If not found in contributions, check team membership directly
            let membership = sqlx::query!(
                r#"
                SELECT team_id 
                FROM team_members 
                WHERE user_id = $1 
                AND status = 'active'
                AND (team_id = $2 OR team_id = $3)
                "#,
                user_id,
                live_game.home_team_id,
                live_game.away_team_id
            )
            .fetch_optional(pool)
            .await?;
            
            match membership {
                Some(m) => Ok(m.team_id),
                None => Err("User does not belong to either team in this game".into())
            }
        }
    }
}

