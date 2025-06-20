use actix_web::{web, HttpResponse, Result};
use sqlx::{PgPool, Row};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use crate::handlers::admin::user_handler::ApiResponse;

#[derive(Serialize)]
pub struct AdminLeagueResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub max_teams: i32,
    pub current_team_count: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct CreateLeagueRequest {
    pub name: String,
    pub description: Option<String>,
    pub max_teams: i32,
}

#[derive(Deserialize)]
pub struct UpdateLeagueRequest {
    pub name: Option<String>,
    pub season_start_date: Option<DateTime<Utc>>,
    pub season_end_date: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct AssignTeamRequest {
    pub team_id: Uuid,
}

#[derive(Deserialize)]
pub struct RemoveTeamRequest {
    pub team_id: Uuid,
}

#[derive(Deserialize)]
pub struct GenerateScheduleRequest {
    pub season_id: Uuid,
    pub start_date: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct CreateSeasonRequest {
    pub name: String,
    pub start_date: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct UpdateSeasonRequest {
    pub name: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct AdminSeasonResponse {
    pub id: Uuid,
    pub league_id: Uuid,
    pub name: String,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub total_teams: i64,
    pub games_count: i64,
    pub created_at: DateTime<Utc>,
}

// GET /admin/leagues - Get all leagues
pub async fn get_leagues(
    pool: web::Data<PgPool>,
) -> Result<HttpResponse> {
    let rows = sqlx::query(r#"
        SELECT 
            l.id,
            l.name,
            l.description,
            l.max_teams,
            l.created_at,
            COUNT(DISTINCT lm.team_id) as current_team_count
        FROM leagues l
        LEFT JOIN league_memberships lm ON l.id = lm.league_id AND lm.status = 'active'
        GROUP BY l.id, l.name, l.description, l.max_teams, l.created_at
        ORDER BY l.created_at DESC
    "#)
    .fetch_all(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error getting leagues: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    let leagues: Vec<AdminLeagueResponse> = rows
        .into_iter()
        .map(|row| AdminLeagueResponse {
            id: row.get("id"),
            name: row.get("name"),
            description: row.get("description"),
            max_teams: row.get("max_teams"),
            current_team_count: row.get::<i64, _>("current_team_count"),
            created_at: row.get("created_at"),
        })
        .collect();

    let response = ApiResponse {
        data: leagues,
        success: true,
        message: None,
    };

    Ok(HttpResponse::Ok().json(response))
}

// GET /admin/leagues/{id} - Get league by ID
pub async fn get_league_by_id(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let league_id = path.into_inner();

    let row = sqlx::query(r#"
        SELECT 
            l.id,
            l.name,
            l.description,
            l.max_teams,
            l.created_at,
            ls.start_date as season_start_date,
            ls.end_date as season_end_date,
            COUNT(DISTINCT lm.team_id) as current_team_count
        FROM leagues l
        LEFT JOIN league_seasons ls ON l.id = ls.league_id
        LEFT JOIN league_memberships lm ON l.id = lm.league_id AND lm.status = 'active'
        WHERE l.id = $1
        GROUP BY l.id, l.name, l.description, l.max_teams, l.created_at, ls.start_date, ls.end_date
    "#)
    .bind(league_id)
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error getting league: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    if let Some(row) = row {
        let league = AdminLeagueResponse {
            id: row.get("id"),
            name: row.get("name"),
            description: row.get("description"),
            max_teams: row.get("max_teams"),
            current_team_count: row.get::<i64, _>("current_team_count"),
            created_at: row.get("created_at"),
        };

        let response = ApiResponse {
            data: league,
            success: true,
            message: None,
        };

        Ok(HttpResponse::Ok().json(response))
    } else {
        Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "League not found"
        })))
    }
}

// POST /admin/leagues - Create new league
pub async fn create_league(
    pool: web::Data<PgPool>,
    body: web::Json<CreateLeagueRequest>,
) -> Result<HttpResponse> {
    // Validate max_teams
    if body.max_teams <= 0 {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "max_teams must be greater than 0"
        })));
    }

    let league_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    // Create league only (seasons will be managed separately)
    let league_result = sqlx::query!(
        r#"
        INSERT INTO leagues (id, name, description, max_teams, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
        league_id,
        body.name,
        body.description,
        body.max_teams,
        now,
        now
    )
    .execute(pool.get_ref())
    .await;

    match league_result {
        Ok(_) => {
            let league = AdminLeagueResponse {
                id: league_id,
                name: body.name.clone(),
                description: body.description.clone(),
                max_teams: body.max_teams,
                current_team_count: 0,
                created_at: now,
            };

            let response = ApiResponse {
                data: league,
                success: true,
                message: Some("League created successfully. Create a season to get started.".to_string()),
            };

            Ok(HttpResponse::Created().json(response))
        }
        Err(e) => {
            eprintln!("Database error creating league: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to create league"
            })))
        }
    }
}

// PATCH /admin/leagues/{id} - Update league
pub async fn update_league(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    body: web::Json<UpdateLeagueRequest>,
) -> Result<HttpResponse> {
    let league_id = path.into_inner();

    if body.name.is_none() && body.season_start_date.is_none() && body.season_end_date.is_none() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "No fields to update"
        })));
    }

    let mut tx = pool.begin().await.map_err(|e| {
        eprintln!("Database error starting transaction: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    let now = chrono::Utc::now();

    // Update leagues table
    let mut league_query_builder = sqlx::QueryBuilder::new("UPDATE leagues SET updated_at = ");
    league_query_builder.push_bind(now);

    if let Some(name) = &body.name {
        league_query_builder.push(", name = ");
        league_query_builder.push_bind(name);
    }

    league_query_builder.push(" WHERE id = ");
    league_query_builder.push_bind(league_id);

    let league_result = league_query_builder.build().execute(&mut *tx).await;

    match league_result {
        Ok(result) => {
            if result.rows_affected() == 0 {
                return Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "League not found"
                })));
            }
        }
        Err(e) => {
            eprintln!("Database error updating league: {}", e);
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to update league"
            })));
        }
    }

    // Update league_seasons table for season-specific fields
    if body.season_start_date.is_some() || body.season_end_date.is_some() {
        let mut season_query_builder = sqlx::QueryBuilder::new("UPDATE league_seasons SET updated_at = ");
        season_query_builder.push_bind(now);

        if let Some(start_date) = &body.season_start_date {
            season_query_builder.push(", start_date = ");
            season_query_builder.push_bind(start_date);
        }

        if let Some(end_date) = &body.season_end_date {
            season_query_builder.push(", end_date = ");
            season_query_builder.push_bind(end_date);
        }

        season_query_builder.push(" WHERE league_id = ");
        season_query_builder.push_bind(league_id);

        let season_result = season_query_builder.build().execute(&mut *tx).await;

        if let Err(e) = season_result {
            eprintln!("Database error updating league season: {}", e);
            return Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to update league season"
            })));
        }
    }

    tx.commit().await.map_err(|e| {
        eprintln!("Database error committing transaction: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    // Fetch updated league
    get_league_by_id(pool, web::Path::from(league_id)).await
}

// POST /admin/leagues/{id}/teams - Assign team to league
pub async fn assign_team_to_league(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    body: web::Json<AssignTeamRequest>,
) -> Result<HttpResponse> {
    let league_id = path.into_inner();
    let team_id = body.team_id;

    // Check if league exists
    let league_exists = sqlx::query!(
        "SELECT id FROM leagues WHERE id = $1",
        league_id
    )
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error checking league: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    if league_exists.is_none() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "League not found"
        })));
    }

    // Check if team exists
    let team_exists = sqlx::query!(
        "SELECT id FROM teams WHERE id = $1",
        team_id
    )
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error checking team: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    if team_exists.is_none() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Team not found"
        })));
    }

    // Check if team is already assigned to this league
    let existing_membership = sqlx::query!(
        "SELECT id FROM league_memberships WHERE league_id = $1 AND team_id = $2",
        league_id,
        team_id
    )
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error checking existing membership: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    if existing_membership.is_some() {
        return Ok(HttpResponse::Conflict().json(serde_json::json!({
            "error": "Team is already assigned to this league"
        })));
    }

    // Insert team into league_memberships only
    // Teams will be added to specific seasons separately when seasons are created/activated
    let result = sqlx::query!(
        r#"
        INSERT INTO league_memberships (id, league_id, team_id, joined_at, status)
        VALUES ($1, $2, $3, NOW(), 'active')
        "#,
        Uuid::new_v4(),
        league_id,
        team_id
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => {

            let response = ApiResponse {
                data: serde_json::json!({
                    "league_id": league_id,
                    "team_id": team_id,
                    "message": "Team assigned to league successfully. Team will be added to seasons when they are created/activated."
                }),
                success: true,
                message: Some("Team assigned to league successfully".to_string()),
            };
            Ok(HttpResponse::Created().json(response))
        }
        Err(e) => {
            eprintln!("Database error assigning team to league: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to assign team to league"
            })))
        }
    }
}

// DELETE /admin/leagues/{id}/teams - Remove team from league
pub async fn remove_team_from_league(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    body: web::Json<RemoveTeamRequest>,
) -> Result<HttpResponse> {
    let league_id = path.into_inner();
    let team_id = body.team_id;

    // Remove team from league_memberships, league_teams, and league_standings tables
    let mut tx = pool.begin().await.map_err(|e| {
        eprintln!("Database error starting transaction: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    // Get all seasons for this league to clean up all data
    let seasons = sqlx::query!(
        "SELECT id FROM league_seasons WHERE league_id = $1",
        league_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("Database error getting seasons: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    let mut total_rows_affected = 0;

    // Remove from league_standings for all seasons
    for season in &seasons {
        let standings_result = sqlx::query!(
            "DELETE FROM league_standings WHERE season_id = $1 AND team_id = $2",
            season.id,
            team_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            eprintln!("Database error removing from league_standings: {}", e);
            actix_web::error::ErrorInternalServerError("Database error")
        })?;
        total_rows_affected += standings_result.rows_affected();
    }

    // Remove from league_teams for all seasons
    for season in &seasons {
        let teams_result = sqlx::query!(
            "DELETE FROM league_teams WHERE season_id = $1 AND team_id = $2",
            season.id,
            team_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            eprintln!("Database error removing from league_teams: {}", e);
            actix_web::error::ErrorInternalServerError("Database error")
        })?;
        total_rows_affected += teams_result.rows_affected();
    }

    // Remove from league_memberships
    let membership_result = sqlx::query!(
        "DELETE FROM league_memberships WHERE league_id = $1 AND team_id = $2",
        league_id,
        team_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("Database error removing from league_memberships: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;
    total_rows_affected += membership_result.rows_affected();

    if total_rows_affected > 0 {
        tx.commit().await.map_err(|e| {
            eprintln!("Database error committing transaction: {}", e);
            actix_web::error::ErrorInternalServerError("Database error")
        })?;

        let response = ApiResponse {
            data: serde_json::json!({
                "league_id": league_id,
                "team_id": team_id
            }),
            success: true,
            message: Some("Team removed from league successfully".to_string()),
        };
        Ok(HttpResponse::Ok().json(response))
    } else {
        Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Team not found in this league"
        })))
    }
}

// GET /admin/leagues/{id}/teams - Get teams assigned to a league
pub async fn get_league_teams(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let league_id = path.into_inner();

    let rows = sqlx::query!(
        r#"
        SELECT 
            t.id,
            t.team_name as name,
            t.team_color as color,
            t.created_at,
            t.user_id as owner_id,
            COUNT(tm.user_id) as member_count,
            COALESCE(SUM(ua.stamina + ua.strength), 0) as total_power
        FROM league_memberships lm
        JOIN teams t ON lm.team_id = t.id
        LEFT JOIN team_members tm ON t.id = tm.team_id AND tm.status = 'active'
        LEFT JOIN user_avatars ua ON tm.user_id = ua.user_id
        WHERE lm.league_id = $1 AND lm.status = 'active'
        GROUP BY t.id, t.team_name, t.team_color, t.created_at, t.user_id
        ORDER BY t.team_name ASC
        "#,
        league_id
    )
    .fetch_all(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error getting league teams: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    let teams: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|row| serde_json::json!({
            "id": row.id,
            "name": row.name,
            "color": row.color,
            "member_count": row.member_count,
            "max_members": 5,
            "total_power": row.total_power,
            "created_at": row.created_at,
            "owner_id": row.owner_id
        }))
        .collect();

    let response = ApiResponse {
        data: teams,
        success: true,
        message: None,
    };

    Ok(HttpResponse::Ok().json(response))
}

// GET /admin/leagues/{league_id}/seasons - Get all seasons for a league
#[tracing::instrument(
    name = "Get league seasons",
    skip(pool),
    fields(league_id = %path.as_ref())
)]
pub async fn get_league_seasons(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let league_id = path.into_inner();
    
    let rows = sqlx::query!(
        r#"
        SELECT 
            ls.id,
            ls.league_id,
            ls.name,
            ls.start_date,
            ls.end_date,
            ls.created_at,
            COUNT(DISTINCT lt.team_id) as total_teams,
            COUNT(DISTINCT lg.id) as games_count
        FROM league_seasons ls
        LEFT JOIN league_teams lt ON ls.id = lt.season_id
        LEFT JOIN league_games lg ON ls.id = lg.season_id
        WHERE ls.league_id = $1
        GROUP BY ls.id, ls.league_id, ls.name, ls.start_date, ls.end_date, ls.created_at
        ORDER BY ls.created_at DESC
        "#,
        league_id
    )
    .fetch_all(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error getting league seasons: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    let seasons: Vec<AdminSeasonResponse> = rows
        .into_iter()
        .map(|row| AdminSeasonResponse {
            id: row.id,
            league_id: row.league_id,
            name: row.name,
            start_date: row.start_date,
            end_date: row.end_date,
            total_teams: row.total_teams.unwrap_or(0),
            games_count: row.games_count.unwrap_or(0),
            created_at: row.created_at,
        })
        .collect();

    let response = ApiResponse {
        data: seasons,
        success: true,
        message: None,
    };

    Ok(HttpResponse::Ok().json(response))
}

// POST /admin/leagues/{league_id}/seasons - Create new season for a league
pub async fn create_league_season(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    body: web::Json<CreateSeasonRequest>,
) -> Result<HttpResponse> {
    let league_id = path.into_inner();
    let season_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    // Validate start date is in the future
    if body.start_date <= now {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Season start date must be in the future"
        })));
    }

    // Validate start date is a Saturday at 22:00 UTC (schedule requirement)
    let countdown_service = crate::league::countdown::CountdownService::new();
    if !countdown_service.is_valid_game_time(body.start_date) {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Start date must be a Saturday at 22:00 UTC"
        })));
    }

    // Calculate end date automatically based on league size
    let team_count = sqlx::query!(
        "SELECT COUNT(*) as count FROM league_memberships WHERE league_id = $1 AND status = 'active'",
        league_id
    )
    .fetch_one(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error counting teams: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?
    .count.unwrap_or(0);

    if team_count < 2 {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "League must have at least 2 teams to create a season"
        })));
    }

    if team_count % 2 != 0 {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "League must have an even number of teams to create a season"
        })));
    }

    // Calculate total games: each team plays every other team twice (home & away)
    // Formula: n * (n-1) where n = number of teams
    // Calculate end date: N/2 games per week, so total weeks = 2*(N-1)
    // Formula: total_games ÷ games_per_week = N*(N-1) ÷ (N/2) = 2*(N-1)

    // Teams are guaranteed to be even due to validation
    let total_weeks = 2 * (team_count - 1);
    let calculated_end_date = body.start_date + chrono::Duration::weeks(total_weeks as i64);

    // Use calculated end date instead of user input
    let end_date = calculated_end_date;

    // Check if league exists
    let league_exists = sqlx::query!(
        "SELECT id FROM leagues WHERE id = $1",
        league_id
    )
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error checking league: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    if league_exists.is_none() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "League not found"
        })));
    }

    // Create the season in a transaction so we can add teams
    let mut tx = pool.begin().await.map_err(|e| {
        eprintln!("Database error starting transaction: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    let result = sqlx::query!(
        r#"
        INSERT INTO league_seasons (id, league_id, name, start_date, end_date, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        season_id,
        league_id,
        body.name,
        body.start_date,
        end_date,
        now,
        now
    )
    .execute(&mut *tx)
    .await;

    match result {
        Ok(_) => {
            // Add all existing league teams to this season
            let teams_added = sqlx::query!(
                r#"
                INSERT INTO league_teams (id, season_id, team_id, joined_at)
                SELECT gen_random_uuid(), $1, lm.team_id, NOW()
                FROM league_memberships lm 
                WHERE lm.league_id = $2 AND lm.status = 'active'
                "#,
                season_id,
                league_id
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                eprintln!("Database error adding teams to season: {}", e);
                actix_web::error::ErrorInternalServerError("Database error")
            })?;

            // Add initial standings for all teams
            sqlx::query!(
                r#"
                INSERT INTO league_standings (id, season_id, team_id, games_played, wins, draws, losses, position, last_updated)
                SELECT gen_random_uuid(), $1, lm.team_id, 0, 0, 0, 0, 1, NOW()
                FROM league_memberships lm 
                WHERE lm.league_id = $2 AND lm.status = 'active'
                "#,
                season_id,
                league_id
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                eprintln!("Database error adding team standings: {}", e);
                actix_web::error::ErrorInternalServerError("Database error")
            })?;

            // Commit the transaction first to ensure season and teams are created
            tx.commit().await.map_err(|e| {
                eprintln!("Database error committing transaction: {}", e);
                actix_web::error::ErrorInternalServerError("Database error")
            })?;

            // Generate games automatically after season creation (date already validated)
            let team_ids_result = sqlx::query!(
                r#"
                SELECT t.id as team_id 
                FROM teams t
                JOIN league_teams lt ON t.id = lt.team_id
                WHERE lt.season_id = $1
                "#,
                season_id
            )
            .fetch_all(pool.get_ref())
            .await;

            let mut games_created = 0;
            if let Ok(teams) = team_ids_result {
                let team_ids: Vec<Uuid> = teams.into_iter().map(|t| t.team_id).collect();
                
                if team_ids.len() >= 2 {
                    let schedule_service = crate::league::schedule::ScheduleService::new(pool.get_ref().clone());
                    
                    match schedule_service.generate_schedule(season_id, &team_ids, body.start_date).await {
                        Ok(created) => {
                            games_created = created;
                            tracing::info!("Automatically generated {} games for new season {}", created, season_id);
                        }
                        Err(e) => {
                            tracing::error!("Failed to automatically generate schedule for season {}: {}", season_id, e);
                            // Schedule generation should not fail since we pre-validated the date
                            // If it does fail, log but don't fail the season creation
                        }
                    }
                }
            }

            let season = AdminSeasonResponse {
                id: season_id,
                league_id,
                name: body.name.clone(),
                start_date: body.start_date,
                end_date: end_date,
                total_teams: teams_added.rows_affected() as i64,
                games_count: games_created as i64,
                created_at: now,
            };

            let response = ApiResponse {
                data: season,
                success: true,
                message: Some(format!("Season created successfully with {} games automatically generated", games_created)),
            };

            Ok(HttpResponse::Created().json(response))
        }
        Err(e) => {
            eprintln!("Database error creating season: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to create season"
            })))
        }
    }
}

// GET /admin/leagues/{league_id}/seasons/{season_id} - Get specific season
pub async fn get_league_season_by_id(
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse> {
    let (league_id, season_id) = path.into_inner();

    let row = sqlx::query!(
        r#"
        SELECT 
            ls.id,
            ls.league_id,
            ls.name,
            ls.start_date,
            ls.end_date,
            ls.created_at,
            COUNT(DISTINCT lt.team_id) as total_teams,
            COUNT(DISTINCT lg.id) as games_count
        FROM league_seasons ls
        LEFT JOIN league_teams lt ON ls.id = lt.season_id
        LEFT JOIN league_games lg ON ls.id = lg.season_id
        WHERE ls.league_id = $1 AND ls.id = $2
        GROUP BY ls.id, ls.league_id, ls.name, ls.start_date, ls.end_date, ls.created_at
        "#,
        league_id,
        season_id
    )
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error getting season: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    if let Some(row) = row {
        let season = AdminSeasonResponse {
            id: row.id,
            league_id: row.league_id,
            name: row.name,
            start_date: row.start_date,
            end_date: row.end_date,
            total_teams: row.total_teams.unwrap_or(0),
            games_count: row.games_count.unwrap_or(0),
            created_at: row.created_at,
        };

        let response = ApiResponse {
            data: season,
            success: true,
            message: None,
        };

        Ok(HttpResponse::Ok().json(response))
    } else {
        Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Season not found"
        })))
    }
}

// PATCH /admin/leagues/{league_id}/seasons/{season_id} - Update season
pub async fn update_league_season(
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<UpdateSeasonRequest>,
) -> Result<HttpResponse> {
    let (league_id, season_id) = path.into_inner();

    if body.name.is_none() && body.start_date.is_none() {
        return Ok(HttpResponse::BadRequest().json(serde_json::json!({
            "error": "No fields to update"
        })));
    }

    let now = chrono::Utc::now();

    // Build dynamic update query
    let mut query_builder = sqlx::QueryBuilder::new("UPDATE league_seasons SET updated_at = ");
    query_builder.push_bind(now);

    if let Some(name) = &body.name {
        query_builder.push(", name = ");
        query_builder.push_bind(name);
    }

    if let Some(start_date) = &body.start_date {
        query_builder.push(", start_date = ");
        query_builder.push_bind(start_date);
    }

    query_builder.push(" WHERE league_id = ");
    query_builder.push_bind(league_id);
    query_builder.push(" AND id = ");
    query_builder.push_bind(season_id);

    let result = query_builder.build().execute(pool.get_ref()).await;

    match result {
        Ok(result) => {
            if result.rows_affected() == 0 {
                return Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Season not found"
                })));
            }

            // Return updated season
            get_league_season_by_id(pool, web::Path::from((league_id, season_id))).await
        }
        Err(e) => {
            eprintln!("Database error updating season: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to update season"
            })))
        }
    }
}

// DELETE /admin/leagues/{league_id}/seasons/{season_id} - Delete season
pub async fn delete_league_season(
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse> {
    let (league_id, season_id) = path.into_inner();

    // Check if season exists and belongs to the league
    let season_exists = sqlx::query!(
        "SELECT id FROM league_seasons WHERE league_id = $1 AND id = $2",
        league_id,
        season_id
    )
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error checking season: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    if season_exists.is_none() {
        return Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Season not found"
        })));
    }

    // Start transaction to delete all related data
    let mut tx = pool.begin().await.map_err(|e| {
        eprintln!("Database error starting transaction: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    // Delete in the correct order to maintain referential integrity
    
    // 1. Delete league games
    sqlx::query!(
        "DELETE FROM league_games WHERE season_id = $1",
        season_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("Database error deleting games: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    // 2. Delete league standings
    sqlx::query!(
        "DELETE FROM league_standings WHERE season_id = $1",
        season_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("Database error deleting standings: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    // 3. Delete league teams
    sqlx::query!(
        "DELETE FROM league_teams WHERE season_id = $1",
        season_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("Database error deleting league teams: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    // 4. Finally delete the season itself
    let result = sqlx::query!(
        "DELETE FROM league_seasons WHERE league_id = $1 AND id = $2",
        league_id,
        season_id
    )
    .execute(&mut *tx)
    .await;

    match result {
        Ok(result) => {
            if result.rows_affected() == 0 {
                return Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Season not found"
                })));
            }

            tx.commit().await.map_err(|e| {
                eprintln!("Database error committing transaction: {}", e);
                actix_web::error::ErrorInternalServerError("Database error")
            })?;

            let response = ApiResponse {
                data: serde_json::json!({
                    "league_id": league_id,
                    "season_id": season_id,
                    "message": "Season deleted successfully"
                }),
                success: true,
                message: Some("Season and all related data deleted successfully".to_string()),
            };

            Ok(HttpResponse::Ok().json(response))
        }
        Err(e) => {
            eprintln!("Database error deleting season: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to delete season"
            })))
        }
    }
}