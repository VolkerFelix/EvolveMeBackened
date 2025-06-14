use actix_web::{web, HttpResponse, Result};
use sqlx::{PgPool, Row};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Serialize)]
pub struct AdminUserResponse {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub stats: UserStats,
    pub total_stats: i32,
    pub is_online: bool,
    pub avatar_style: String,
    pub team_id: Option<Uuid>,
    pub team_role: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub last_active_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct UserStats {
    pub stamina: i32,
    pub strength: i32,
}

#[derive(Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}

#[derive(Serialize)]
pub struct PaginationInfo {
    pub page: i32,
    pub limit: i32,
    pub total: i64,
    pub total_pages: i32,
}

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub data: T,
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Deserialize)]
pub struct UserQueryParams {
    pub page: Option<i32>,
    pub limit: Option<i32>,
    pub search: Option<String>,
    pub team_id: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateUserStatusRequest {
    pub status: String,
}

// GET /admin/users - List users with pagination and filtering
pub async fn get_users(
    pool: web::Data<PgPool>,
    query: web::Query<UserQueryParams>,
) -> Result<HttpResponse> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).min(100);
    let offset = (page - 1) * limit;

    let mut sql = r#"
        SELECT 
            u.id,
            u.username,
            u.email,
            u.created_at,
            ua.stamina,
            ua.strength,
            ua.avatar_style,
            tm.team_id,
            tm.role as team_role,
            COALESCE(ua.stamina + ua.strength, 0) as total_stats,
            u.updated_at as last_active_at
        FROM users u
        LEFT JOIN user_avatars ua ON u.id = ua.user_id
        LEFT JOIN team_members tm ON u.id = tm.user_id AND tm.status = 'active'
        WHERE 1=1
    "#.to_string();

    let mut count_sql = r#"
        SELECT COUNT(*)
        FROM users u
        LEFT JOIN user_avatars ua ON u.id = ua.user_id
        LEFT JOIN team_members tm ON u.id = tm.user_id AND tm.status = 'active'
        WHERE 1=1
    "#.to_string();

    // Add search filter
    if let Some(search) = &query.search {
        if !search.is_empty() {
            sql.push_str(&format!(
                " AND (u.username ILIKE '%{}%' OR u.email ILIKE '%{}%')",
                search.replace('\'', "''"),
                search.replace('\'', "''")
            ));
            count_sql.push_str(&format!(
                " AND (u.username ILIKE '%{}%' OR u.email ILIKE '%{}%')",
                search.replace('\'', "''"),
                search.replace('\'', "''")
            ));
        }
    }

    // Add team filter
    if let Some(team_filter) = &query.team_id {
        if team_filter == "null" {
            sql.push_str(" AND tm.team_id IS NULL");
            count_sql.push_str(" AND tm.team_id IS NULL");
        } else if let Ok(team_uuid) = Uuid::parse_str(team_filter) {
            sql.push_str(&format!(" AND tm.team_id = '{}'", team_uuid));
            count_sql.push_str(&format!(" AND tm.team_id = '{}'", team_uuid));
        }
    }

    sql.push_str(&format!(
        " ORDER BY u.created_at DESC LIMIT {} OFFSET {}",
        limit, offset
    ));

    // Get total count
    let total_count: (i64,) = sqlx::query_as(&count_sql)
        .fetch_one(pool.get_ref())
        .await
        .map_err(|e| {
            eprintln!("Database error getting user count: {}", e);
            actix_web::error::ErrorInternalServerError("Database error")
        })?;

    // Get users
    let rows = sqlx::query(&sql)
        .fetch_all(pool.get_ref())
        .await
        .map_err(|e| {
            eprintln!("Database error getting users: {}", e);
            actix_web::error::ErrorInternalServerError("Database error")
        })?;

    let users: Vec<AdminUserResponse> = rows
        .into_iter()
        .map(|row| AdminUserResponse {
            id: row.get("id"),
            username: row.get("username"),
            email: row.get("email"),
            stats: UserStats {
                stamina: row.get::<Option<i32>, _>("stamina").unwrap_or(0),
                strength: row.get::<Option<i32>, _>("strength").unwrap_or(0),
            },
            total_stats: row.get::<Option<i32>, _>("total_stats").unwrap_or(0),
            is_online: false, // TODO: Implement real online status
            avatar_style: row.get::<Option<String>, _>("avatar_style").unwrap_or_else(|| "warrior".to_string()),
            team_id: row.get("team_id"),
            team_role: row.get("team_role"),
            status: "active".to_string(), // TODO: Add status field to users table
            created_at: row.get("created_at"),
            last_active_at: row.get("last_active_at"),
        })
        .collect();

    let total_pages = ((total_count.0 as f64) / (limit as f64)).ceil() as i32;

    let response = PaginatedResponse {
        data: users,
        pagination: PaginationInfo {
            page,
            limit,
            total: total_count.0,
            total_pages,
        },
    };

    Ok(HttpResponse::Ok().json(response))
}

// GET /admin/users/{id} - Get user by ID
pub async fn get_user_by_id(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse> {
    let user_id = path.into_inner();

    let row = sqlx::query(r#"
        SELECT 
            u.id,
            u.username,
            u.email,
            u.created_at,
            ua.stamina,
            ua.strength,
            ua.avatar_style,
            tm.team_id,
            tm.role as team_role,
            COALESCE(ua.stamina + ua.strength, 0) as total_stats,
            u.updated_at as last_active_at
        FROM users u
        LEFT JOIN user_avatars ua ON u.id = ua.user_id
        LEFT JOIN team_members tm ON u.id = tm.user_id AND tm.status = 'active'
        WHERE u.id = $1
    "#)
    .bind(user_id)
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error getting user: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    if let Some(row) = row {
        let user = AdminUserResponse {
            id: row.get("id"),
            username: row.get("username"),
            email: row.get("email"),
            stats: UserStats {
                stamina: row.get::<Option<i32>, _>("stamina").unwrap_or(0),
                strength: row.get::<Option<i32>, _>("strength").unwrap_or(0),
            },
            total_stats: row.get::<Option<i32>, _>("total_stats").unwrap_or(0),
            is_online: false,
            avatar_style: row.get::<Option<String>, _>("avatar_style").unwrap_or_else(|| "warrior".to_string()),
            team_id: row.get("team_id"),
            team_role: row.get("team_role"),
            status: "active".to_string(),
            created_at: row.get("created_at"),
            last_active_at: row.get("last_active_at"),
        };

        let response = ApiResponse {
            data: user,
            success: true,
            message: None,
        };

        Ok(HttpResponse::Ok().json(response))
    } else {
        Ok(HttpResponse::NotFound().json(serde_json::json!({
            "error": "User not found"
        })))
    }
}

// PATCH /admin/users/{id}/status - Update user status
pub async fn update_user_status(
    _pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    body: web::Json<UpdateUserStatusRequest>,
) -> Result<HttpResponse> {
    let user_id = path.into_inner();
    
    // TODO: Implement user status updates when status field is added to users table
    // For now, just return success
    
    let response = ApiResponse {
        data: serde_json::json!({"id": user_id, "status": body.status}),
        success: true,
        message: Some("Status update functionality coming soon".to_string()),
    };

    Ok(HttpResponse::Ok().json(response))
}

// GET /admin/users/without-team - Get users without teams
pub async fn get_users_without_team(
    pool: web::Data<PgPool>,
) -> Result<HttpResponse> {
    let rows = sqlx::query(r#"
        SELECT 
            u.id,
            u.username,
            u.email,
            u.created_at,
            ua.stamina,
            ua.strength,
            ua.avatar_style,
            COALESCE(ua.stamina + ua.strength, 0) as total_stats,
            u.updated_at as last_active_at
        FROM users u
        LEFT JOIN user_avatars ua ON u.id = ua.user_id
        LEFT JOIN team_members tm ON u.id = tm.user_id AND tm.status = 'active'
        WHERE tm.team_id IS NULL
        ORDER BY u.created_at DESC
    "#)
    .fetch_all(pool.get_ref())
    .await
    .map_err(|e| {
        eprintln!("Database error getting users without team: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })?;

    let users: Vec<AdminUserResponse> = rows
        .into_iter()
        .map(|row| AdminUserResponse {
            id: row.get("id"),
            username: row.get("username"),
            email: row.get("email"),
            stats: UserStats {
                stamina: row.get::<Option<i32>, _>("stamina").unwrap_or(0),
                strength: row.get::<Option<i32>, _>("strength").unwrap_or(0),
            },
            total_stats: row.get::<Option<i32>, _>("total_stats").unwrap_or(0),
            is_online: false,
            avatar_style: row.get::<Option<String>, _>("avatar_style").unwrap_or_else(|| "warrior".to_string()),
            team_id: None,
            team_role: None,
            status: "active".to_string(),
            created_at: row.get("created_at"),
            last_active_at: row.get("last_active_at"),
        })
        .collect();

    let response = ApiResponse {
        data: users,
        success: true,
        message: None,
    };

    Ok(HttpResponse::Ok().json(response))
}