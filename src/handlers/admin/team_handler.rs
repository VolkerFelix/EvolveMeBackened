use actix_web::{web, HttpResponse, Result};
use sqlx::{PgPool, Row};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use crate::handlers::admin::user_handler::{PaginatedResponse, PaginationInfo, ApiResponse};

#[derive(Serialize)]
pub struct AdminTeamResponse {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    pub member_count: i64,
    pub max_members: i32,
    pub total_power: i64,
    pub formation: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub owner_id: Uuid,
    pub league_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct AdminTeamMemberResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub team_id: Uuid,
    pub role: String,
    pub status: String,
    pub joined_at: DateTime<Utc>,
    pub user: AdminUserInfo,
}

#[derive(Serialize)]
pub struct AdminUserInfo {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub stats: UserStats,
    pub total_stats: i32,
    pub is_online: bool,
    pub avatar_style: String,
}

#[derive(Serialize)]
pub struct UserStats {
    pub stamina: i32,
    pub strength: i32,
}

#[derive(Deserialize)]
pub struct TeamQueryParams {
    pub page: Option<i32>,
    pub limit: Option<i32>,
    pub search: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateTeamRequest {
    pub name: String,
    pub color: String,
    pub formation: Option<String>,
    pub league_id: Option<Uuid>,
    pub owner_id: Uuid,
}

#[derive(Deserialize)]
pub struct AddTeamMemberRequest {
    pub user_id: Uuid,
    pub role: String,
}

#[derive(Deserialize)]
pub struct UpdateTeamMemberRequest {
    pub role: Option<String>,
    pub status: Option<String>,
}

// GET /admin/teams - List teams with pagination
pub async fn get_teams(
    pool: web::Data<PgPool>,
    query: web::Query<TeamQueryParams>,
) -> Result<HttpResponse> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = (page - 1) * limit;

    let mut sql = r#"
        SELECT 
            t.id,
            t.team_name as name,
            t.team_color as color,
            t.created_at,
            t.user_id as owner_id,
            COUNT(tm.user_id) as member_count,
            COALESCE(SUM(ua.stamina + ua.strength), 0) as total_power
        FROM teams t
        LEFT JOIN team_members tm ON t.id = tm.team_id AND tm.status = 'active'
        LEFT JOIN user_avatars ua ON tm.user_id = ua.user_id
        WHERE 1=1
    "#.to_string();

    let mut count_sql = r#"
        SELECT COUNT(DISTINCT t.id)
        FROM teams t
        WHERE 1=1
    "#.to_string();

    // Add search filter
    if let Some(search) = &query.search {
        if !search.is_empty() {
            sql.push_str(&format!(
                " AND t.team_name ILIKE '%{}%'",
                search.replace('\'', "''")
            ));
            count_sql.push_str(&format!(
                " AND t.team_name ILIKE '%{}%'",
                search.replace('\'', "''")
            ));
        }
    }

    sql.push_str(" GROUP BY t.id, t.team_name, t.team_color, t.created_at, t.user_id");
    sql.push_str(&format!(
        " ORDER BY t.created_at DESC LIMIT {} OFFSET {}",
        limit, offset
    ));

    // Get total count
    let total_count: (i64,) = sqlx::query_as(&count_sql)
        .fetch_one(pool.get_ref())
        .await
        .map_err(|e| {
            eprintln!("Database error getting team count: {}", e);
            actix_web::error::ErrorInternalServerError("Database error")
        })?;

    // Get teams
    let rows = sqlx::query(&sql)
        .fetch_all(pool.get_ref())
        .await
        .map_err(|e| {
            eprintln!("Database error getting teams: {}", e);
            actix_web::error::ErrorInternalServerError("Database error")
        })?;

    let teams: Vec<AdminTeamResponse> = rows
        .into_iter()
        .map(|row| AdminTeamResponse {
            id: row.get("id"),
            name: row.get("name"),
            color: row.get("color"),
            member_count: row.get("member_count"),
            max_members: 5, // Default max members
            total_power: row.get("total_power"),
            formation: "circle".to_string(), // Default formation
            is_active: true, // Default active
            created_at: row.get("created_at"),
            owner_id: row.get("owner_id"),
            league_id: None, // TODO: Add league association
        })
        .collect();

    let total_pages = ((total_count.0 as f64) / (limit as f64)).ceil() as i32;

    let response = PaginatedResponse {
        data: teams,
        pagination: PaginationInfo {
            page,
            limit,
            total: total_count.0,
            total_pages,
        },
    };

    Ok(HttpResponse::Ok().json(response))
}

// GET /admin/teams/{id} - Get team by ID
pub async fn get_team_by_id(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let team_id = path.into_inner();

    let row = sqlx::query(r#"
        SELECT 
            t.id,
            t.team_name as name,
            t.team_color as color,
            t.created_at,
            t.user_id as owner_id,
            COUNT(tm.user_id) as member_count,
            COALESCE(SUM(ua.stamina + ua.strength), 0) as total_power
        FROM teams t
        LEFT JOIN team_members tm ON t.id = tm.team_id AND tm.status = 'active'
        LEFT JOIN user_avatars ua ON tm.user_id = ua.user_id
        WHERE t.id = $1
        GROUP BY t.id, t.team_name, t.team_color, t.created_at, t.user_id
    "#)
    .bind(team_id)
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error getting team: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    if let Some(row) = row {
        let team = AdminTeamResponse {
            id: row.get("id"),
            name: row.get("name"),
            color: row.get("color"),
            member_count: row.get("member_count"),
            max_members: 5,
            total_power: row.get("total_power"),
            formation: "circle".to_string(),
            is_active: true,
            created_at: row.get("created_at"),
            owner_id: row.get("owner_id"),
            league_id: None,
        };

        let response = ApiResponse {
            data: team,
            success: true,
            message: None,
        };

        Ok(HttpResponse::Ok().json(response))
    } else {
        Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "Team not found"
        })))
    }
}

// POST /admin/teams - Create new team
pub async fn create_team(
    pool: web::Data<PgPool>,
    body: web::Json<CreateTeamRequest>,
    _req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    // Use the provided owner_id from the request body
    let owner_id = body.owner_id;

    let team_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let result = sqlx::query!(
        r#"
        INSERT INTO teams (id, user_id, team_name, team_description, team_color, team_icon, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        team_id,
        owner_id,
        body.name,
        None::<String>,
        body.color,
        "⚔️",
        now,
        now
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => {
            let team = AdminTeamResponse {
                id: team_id,
                name: body.name.clone(),
                color: body.color.clone(),
                member_count: 0,
                max_members: 5,
                total_power: 0,
                formation: body.formation.clone().unwrap_or_else(|| "circle".to_string()),
                is_active: true,
                created_at: now,
                owner_id,
                league_id: body.league_id,
            };

            let response = ApiResponse {
                data: team,
                success: true,
                message: Some("Team created successfully".to_string()),
            };

            Ok(HttpResponse::Created().json(response))
        }
        Err(e) => {
            eprintln!("Database error creating team: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to create team"
            })))
        }
    }
}

// PATCH /admin/teams/{id} - Update team
pub async fn update_team(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    body: web::Json<CreateTeamRequest>,
) -> Result<HttpResponse> {
    let team_id = path.into_inner();

    let result = sqlx::query!(
        r#"
        UPDATE teams 
        SET team_name = $1, team_color = $2, updated_at = $3
        WHERE id = $4
        "#,
        body.name,
        body.color,
        chrono::Utc::now(),
        team_id
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(result) => {
            if result.rows_affected() > 0 {
                // Fetch updated team
                let team = get_team_by_id(pool, web::Path::from(team_id)).await?;
                Ok(team)
            } else {
                Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Team not found"
                })))
            }
        }
        Err(e) => {
            eprintln!("Database error updating team: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to update team"
            })))
        }
    }
}

// DELETE /admin/teams/{id} - Delete team
pub async fn delete_team(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let team_id = path.into_inner();

    // First, remove all team members
    let _ = sqlx::query!(
        "DELETE FROM team_members WHERE team_id = $1",
        team_id
    )
    .execute(pool.get_ref())
    .await;

    // Then delete the team
    let result = sqlx::query!(
        "DELETE FROM teams WHERE id = $1",
        team_id
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(result) => {
            if result.rows_affected() > 0 {
                let response = ApiResponse {
                    data: serde_json::json!({}),
                    success: true,
                    message: Some("Team deleted successfully".to_string()),
                };
                Ok(HttpResponse::Ok().json(response))
            } else {
                Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Team not found"
                })))
            }
        }
        Err(e) => {
            eprintln!("Database error deleting team: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to delete team"
            })))
        }
    }
}

// GET /admin/teams/{id}/members - Get team members
pub async fn get_team_members(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let team_id = path.into_inner();

    let rows = sqlx::query(r#"
        SELECT 
            tm.id,
            tm.user_id,
            tm.team_id,
            tm.role,
            tm.status,
            tm.joined_at,
            u.username,
            u.email,
            COALESCE(ua.stamina, 0) as stamina,
            COALESCE(ua.strength, 0) as strength,
            COALESCE(ua.stamina + ua.strength, 0) as total_stats,
            COALESCE(ua.avatar_style, 'warrior') as avatar_style
        FROM team_members tm
        JOIN users u ON tm.user_id = u.id
        LEFT JOIN user_avatars ua ON u.id = ua.user_id
        WHERE tm.team_id = $1
        ORDER BY tm.joined_at ASC
    "#)
    .bind(team_id)
    .fetch_all(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error getting team members: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    let members: Vec<AdminTeamMemberResponse> = rows
        .into_iter()
        .map(|row| AdminTeamMemberResponse {
            id: row.get("id"),
            user_id: row.get("user_id"),
            team_id: row.get("team_id"),
            role: row.get("role"),
            status: row.get("status"),
            joined_at: row.get("joined_at"),
            user: AdminUserInfo {
                id: row.get("user_id"),
                username: row.get("username"),
                email: row.get("email"),
                stats: UserStats {
                    stamina: row.get("stamina"),
                    strength: row.get("strength"),
                },
                total_stats: row.get("total_stats"),
                is_online: false, // TODO: Implement real online status
                avatar_style: row.get("avatar_style"),
            },
        })
        .collect();

    let response = ApiResponse {
        data: members,
        success: true,
        message: None,
    };

    Ok(HttpResponse::Ok().json(response))
}

// POST /admin/teams/{id}/members - Add team member
pub async fn add_team_member(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    body: web::Json<AddTeamMemberRequest>,
) -> Result<HttpResponse> {
    let team_id = path.into_inner();
    let member_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    // Parse role
    let role = match body.role.as_str() {
        "admin" => "admin",
        "member" => "member",
        _ => "member", // Default to member
    };

    let result = sqlx::query!(
        r#"
        INSERT INTO team_members (id, team_id, user_id, role, status, joined_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        member_id,
        team_id,
        body.user_id,
        role,
        "active",
        now,
        now
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(_) => {
            // Fetch the created member
            let row = sqlx::query(r#"
                SELECT 
                    tm.id,
                    tm.user_id,
                    tm.team_id,
                    tm.role,
                    tm.status,
                    tm.joined_at,
                    u.username,
                    u.email,
                    COALESCE(ua.stamina, 0) as stamina,
                    COALESCE(ua.strength, 0) as strength,
                    COALESCE(ua.stamina + ua.strength, 0) as total_stats,
                    COALESCE(ua.avatar_style, 'warrior') as avatar_style
                FROM team_members tm
                JOIN users u ON tm.user_id = u.id
                LEFT JOIN user_avatars ua ON u.id = ua.user_id
                WHERE tm.id = $1
            "#)
            .bind(member_id)
            .fetch_one(pool.get_ref())
            .await
            .map_err(|e| {
                eprintln!("Database error fetching created member: {}", e);
                actix_web::error::ErrorInternalServerError("Database error")
            })?;

            let member = AdminTeamMemberResponse {
                id: row.get("id"),
                user_id: row.get("user_id"),
                team_id: row.get("team_id"),
                role: row.get("role"),
                status: row.get("status"),
                joined_at: row.get("joined_at"),
                user: AdminUserInfo {
                    id: row.get("user_id"),
                    username: row.get("username"),
                    email: row.get("email"),
                    stats: UserStats {
                        stamina: row.get("stamina"),
                        strength: row.get("strength"),
                    },
                    total_stats: row.get("total_stats"),
                    is_online: false,
                    avatar_style: row.get("avatar_style"),
                },
            };

            let response = ApiResponse {
                data: member,
                success: true,
                message: Some("Team member added successfully".to_string()),
            };

            Ok(HttpResponse::Created().json(response))
        }
        Err(e) => {
            eprintln!("Database error adding team member: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to add team member"
            })))
        }
    }
}

// PATCH /admin/teams/{team_id}/members/{member_id} - Update team member
pub async fn update_team_member(
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<UpdateTeamMemberRequest>,
) -> Result<HttpResponse> {
    let (team_id, member_id) = path.into_inner();
    let now = chrono::Utc::now();

    // Handle different update combinations using sqlx::query! macro
    let result = match (&body.role, &body.status) {
        (Some(role), Some(status)) => {
            sqlx::query!(
                "UPDATE team_members SET role = $1, status = $2, updated_at = $3 WHERE id = $4 AND team_id = $5",
                role,
                status,
                now,
                member_id,
                team_id
            )
            .execute(pool.get_ref())
            .await
        }
        (Some(role), None) => {
            sqlx::query!(
                "UPDATE team_members SET role = $1, updated_at = $2 WHERE id = $3 AND team_id = $4",
                role,
                now,
                member_id,
                team_id
            )
            .execute(pool.get_ref())
            .await
        }
        (None, Some(status)) => {
            sqlx::query!(
                "UPDATE team_members SET status = $1, updated_at = $2 WHERE id = $3 AND team_id = $4",
                status,
                now,
                member_id,
                team_id
            )
            .execute(pool.get_ref())
            .await
        }
        (None, None) => {
            return Ok(HttpResponse::BadRequest().json(serde_json::json!({
                "error": "No fields to update"
            })));
        }
    };

    match result {
        Ok(result) => {
            if result.rows_affected() > 0 {
                let response = ApiResponse {
                    data: serde_json::json!({"id": member_id, "team_id": team_id}),
                    success: true,
                    message: Some("Team member updated successfully".to_string()),
                };
                Ok(HttpResponse::Ok().json(response))
            } else {
                Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Team member not found"
                })))
            }
        }
        Err(e) => {
            eprintln!("Database error updating team member: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to update team member"
            })))
        }
    }
}

// DELETE /admin/teams/{team_id}/members/{member_id} - Remove team member
pub async fn remove_team_member(
    pool: web::Data<PgPool>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse> {
    let (team_id, member_id) = path.into_inner();

    let result = sqlx::query!(
        "DELETE FROM team_members WHERE id = $1 AND team_id = $2",
        member_id,
        team_id
    )
    .execute(pool.get_ref())
    .await;

    match result {
        Ok(result) => {
            if result.rows_affected() > 0 {
                let response = ApiResponse {
                    data: serde_json::json!({}),
                    success: true,
                    message: Some("Team member removed successfully".to_string()),
                };
                Ok(HttpResponse::Ok().json(response))
            } else {
                Ok(HttpResponse::NotFound().json(serde_json::json!({
                    "error": "Team member not found"
                })))
            }
        }
        Err(e) => {
            eprintln!("Database error removing team member: {}", e);
            Ok(HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to remove team member"
            })))
        }
    }
}