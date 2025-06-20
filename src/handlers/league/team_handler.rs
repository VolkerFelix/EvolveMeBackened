use actix_web::{web, HttpResponse, Result};
use sqlx::PgPool;
use uuid::Uuid;
use serde_json::json;
use chrono::Utc;

use crate::middleware::auth::Claims;
use crate::models::league::*;
use crate::models::team::{TeamRegistrationRequest, TeamUpdateRequest, TeamInfo};

/// Register a new team
#[tracing::instrument(
    name = "Register team",
    skip(team_request, pool, claims),
    fields(
        team_name = %team_request.team_name,
        user = %claims.username
    )
)]
pub async fn register_new_team(
    team_request: web::Json<TeamRegistrationRequest>,
    pool: web::Data<PgPool>,
    claims: web::ReqData<Claims>,
) -> Result<HttpResponse> {
    tracing::info!("Registering team '{}' for user: {}", 
        team_request.team_name, claims.username);

    // Validate the team registration request
    if let Err(validation_error) = team_request.validate() {
        tracing::warn!("Team registration validation failed: {}", validation_error);
        return Ok(HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": validation_error
        })));
    }

    // Parse user ID from claims
    let user_id = match Uuid::parse_str(&claims.sub) {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Invalid user ID in claims: {}", e);
            return Ok(HttpResponse::BadRequest().json(json!({
                "success": false,
                "message": "Invalid user ID"
            })));
        }
    };

    // Check if user already has a team
    match sqlx::query!(
        "SELECT id FROM teams WHERE user_id = $1",
        user_id
    )
    .fetch_optional(pool.get_ref())
    .await
    {
        Ok(Some(_)) => {
            return Ok(HttpResponse::Conflict().json(json!({
                "success": false,
                "message": "User already has a registered team"
            })));
        }
        Ok(None) => {
            // User doesn't have a team yet, proceed with registration
        }
        Err(e) => {
            tracing::error!("Database error checking existing team: {}", e);
            return Ok(HttpResponse::InternalServerError().json(json!({
                "success": false,
                "message": "Failed to check existing team registration"
            })));
        }
    }

    // Check if team name is already taken
    let sanitized_team_name = team_request.get_sanitized_name();
    match sqlx::query!(
        "SELECT id FROM teams WHERE LOWER(team_name) = LOWER($1)",
        sanitized_team_name
    )
    .fetch_optional(pool.get_ref())
    .await
    {
        Ok(Some(_)) => {
            return Ok(HttpResponse::Conflict().json(json!({
                "success": false,
                "message": "Team name already taken"
            })));
        }
        Ok(None) => {
            // Team name is available
        }
        Err(e) => {
            tracing::error!("Database error checking team name: {}", e);
            return Ok(HttpResponse::InternalServerError().json(json!({
                "success": false,
                "message": "Failed to check team name availability"
            })));
        }
    }

    // Create the team
    let team_id = Uuid::new_v4();
    let now = Utc::now();

    // Start a transaction to ensure both team and membership are created atomically
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("Failed to start transaction: {}", e);
            return Ok(HttpResponse::InternalServerError().json(json!({
                "success": false,
                "message": "Failed to start database transaction"
            })));
        }
    };

    // Create the team
    match sqlx::query!(
        r#"
        INSERT INTO teams (id, user_id, team_name, team_description, team_color, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        team_id,
        user_id,
        sanitized_team_name,
        team_request.team_description,
        team_request.team_color.as_deref().unwrap_or("#4F46E5"),
        now,
        now
    )
    .execute(&mut *tx)
    .await
    {
        Ok(_) => {
            tracing::info!("Successfully created team '{}' with ID: {}", 
                team_request.team_name, team_id);
        }
        Err(e) => {
            tracing::error!("Failed to create team: {}", e);
            let _ = tx.rollback().await;
            return Ok(HttpResponse::InternalServerError().json(json!({
                "success": false,
                "message": "Failed to create team"
            })));
        }
    }

    // Add the team owner as a member with 'owner' role
    let member_id = Uuid::new_v4();
    match sqlx::query!(
        r#"
        INSERT INTO team_members (id, team_id, user_id, role, status, joined_at, updated_at)
        VALUES ($1, $2, $3, 'owner', 'active', $4, $5)
        "#,
        member_id,
        team_id,
        user_id,
        now,
        now
    )
    .execute(&mut *tx)
    .await
    {
        Ok(_) => {
            tracing::info!("Successfully added team owner as member for team {}", team_id);
        }
        Err(e) => {
            tracing::error!("Failed to add team owner as member: {}", e);
            let _ = tx.rollback().await;
            return Ok(HttpResponse::InternalServerError().json(json!({
                "success": false,
                "message": "Failed to add team owner as member"
            })));
        }
    }

    // Commit the transaction
    match tx.commit().await {
        Ok(_) => {
            tracing::info!("Successfully registered team '{}' with ID: {} and added owner as member", 
                team_request.team_name, team_id);

            Ok(HttpResponse::Created().json(json!({
                "success": true,
                "message": "Team registered successfully",
                "data": {
                    "team_id": team_id,
                    "team_name": team_request.team_name,
                    "user_id": user_id,
                    "created_at": now
                }
            })))
        }
        Err(e) => {
            tracing::error!("Failed to commit team registration transaction: {}", e);
            Ok(HttpResponse::InternalServerError().json(json!({
                "success": false,
                "message": "Failed to complete team registration"
            })))
        }
    }
}

/// Get team information
pub async fn get_team_information(
    team_id: Uuid,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse> {
    match sqlx::query_as!(
        TeamInfo,
        r#"
        SELECT 
            t.id,
            t.user_id,
            t.team_name,
            t.team_description,
            t.team_color,
            t.created_at,
            t.updated_at,
            u.username as owner_username
        FROM teams t
        JOIN users u ON t.user_id = u.id
        WHERE t.id = $1
        "#,
        team_id
    )
    .fetch_optional(pool.get_ref())
    .await
    {
        Ok(Some(team)) => {
            Ok(HttpResponse::Ok().json(json!({
                "success": true,
                "data": team
            })))
        }
        Ok(None) => {
            Ok(HttpResponse::NotFound().json(json!({
                "success": false,
                "message": "Team not found"
            })))
        }
        Err(e) => {
            tracing::error!("Failed to get team {}: {}", team_id, e);
            Ok(HttpResponse::InternalServerError().json(json!({
                "success": false,
                "message": "Failed to retrieve team information"
            })))
        }
    }
}

/// Get all registered teams
pub async fn get_all_registered_teams(
    query: web::Query<PaginationQuery>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse> {
    let limit = query.limit.unwrap_or(20).min(100);
    
    match sqlx::query_as!(
        TeamInfo,
        r#"
        SELECT 
            t.id,
            t.user_id,
            t.team_name,
            t.team_description,
            t.team_color,
            t.created_at,
            t.updated_at,
            u.username as owner_username
        FROM teams t
        JOIN users u ON t.user_id = u.id
        ORDER BY t.created_at DESC
        LIMIT $1
        "#,
        limit as i64
    )
    .fetch_all(pool.get_ref())
    .await
    {
        Ok(teams) => {
            Ok(HttpResponse::Ok().json(json!({
                "success": true,
                "data": teams,
                "pagination": {
                    "limit": limit,
                    "total": teams.len()
                }
            })))
        }
        Err(e) => {
            tracing::error!("Failed to get teams: {}", e);
            Ok(HttpResponse::InternalServerError().json(json!({
                "success": false,
                "message": "Failed to retrieve teams"
            })))
        }
    }
}

/// Update team information
pub async fn update_team_information(
    team_id: Uuid,
    team_update: web::Json<TeamUpdateRequest>,
    pool: web::Data<PgPool>,
    claims: web::ReqData<Claims>,
) -> Result<HttpResponse> {
    // Validate the update request
    if let Err(validation_error) = team_update.validate() {
        tracing::warn!("Team update validation failed: {}", validation_error);
        return Ok(HttpResponse::BadRequest().json(json!({
            "success": false,
            "message": validation_error
        })));
    }
    
    let user_id = match Uuid::parse_str(&claims.sub) {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Invalid user ID in claims: {}", e);
            return Ok(HttpResponse::BadRequest().json(json!({
                "success": false,
                "message": "Invalid user ID"
            })));
        }
    };

    // Verify user owns this team
    match sqlx::query!(
        "SELECT user_id FROM teams WHERE id = $1",
        team_id
    )
    .fetch_optional(pool.get_ref())
    .await
    {
        Ok(Some(team)) => {
            if team.user_id != user_id {
                return Ok(HttpResponse::Forbidden().json(json!({
                    "success": false,
                    "message": "You can only update your own team"
                })));
            }
        }
        Ok(None) => {
            return Ok(HttpResponse::NotFound().json(json!({
                "success": false,
                "message": "Team not found"
            })));
        }
        Err(e) => {
            tracing::error!("Database error checking team ownership: {}", e);
            return Ok(HttpResponse::InternalServerError().json(json!({
                "success": false,
                "message": "Failed to verify team ownership"
            })));
        }
    }

    // Update team information
    match sqlx::query!(
        r#"
        UPDATE teams 
        SET team_name = COALESCE($1, team_name),
            team_description = COALESCE($2, team_description),
            team_color = COALESCE($3, team_color),
            updated_at = NOW()
        WHERE id = $4
        "#,
        team_update.team_name.as_deref(),
        team_update.team_description.as_deref(),
        team_update.team_color.as_deref(),
        team_id
    )
    .execute(pool.get_ref())
    .await
    {
        Ok(_) => {
            tracing::info!("Successfully updated team {}", team_id);
            Ok(HttpResponse::Ok().json(json!({
                "success": true,
                "message": "Team updated successfully"
            })))
        }
        Err(e) => {
            tracing::error!("Failed to update team {}: {}", team_id, e);
            Ok(HttpResponse::InternalServerError().json(json!({
                "success": false,
                "message": "Failed to update team"
            })))
        }
    }
}

/// Get team league history
pub async fn get_team_league_history(
    _team_id: Uuid,
    _pool: web::Data<PgPool>,
) -> Result<HttpResponse> {
    // Implementation for team history
    Ok(HttpResponse::Ok().json(json!({
        "success": true,
        "data": [],
        "message": "Team history endpoint - implementation needed"
    })))
}