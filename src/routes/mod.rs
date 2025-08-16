use actix_web::web;

pub mod registration;
pub mod backend_health;
pub mod auth;
pub mod protected;
pub mod health_data;
pub mod websocket;
pub mod league;
pub mod profile;
pub mod health_activity;
pub mod admin;

use crate::middleware::auth::AuthMiddleware;

pub fn init_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(registration::register)
        .service(backend_health::backend_health)
        .service(auth::login);

    cfg.service(
        web::scope("/protected")
            .service(protected::protected_resource)
    );
    // Health routes (require authentication)
    cfg.service(
        web::scope("/health")
            .wrap(AuthMiddleware)
            .service(health_data::upload_health)
            .service(health_data::upload_media)
            .service(health_data::update_media)
            .service(health_activity::get_activity_sum)
            .service(health_activity::get_zone_ana)
            .service(health_activity::get_workout_hist)
            .service(health_activity::check_sync_status)
    );
    
    // Public media serving (no auth required)
    cfg.service(
        web::scope("/api")
            .service(health_data::serve_media)
    );
    // Profile routes (require authentication)
    cfg.service(
        web::scope("/profile")
            .wrap(AuthMiddleware)
            .service(profile::get_user)
            .service(profile::get_health_prof)
            .service(profile::update_health_prof)
    );
    // League routes (require authentication)
    cfg.service(
        web::scope("/league")
            .wrap(AuthMiddleware)
            .service(league::create_season)
            .service(league::get_active_season)
            .service(league::get_season)
            .service(league::get_all_seasons)
            .service(league::get_season_schedule)
            .service(league::get_season_standings)
            .service(league::update_game_result)
            .service(league::get_countdown_info)
            .service(league::get_upcoming_games)
            .service(league::get_live_active_games)
            .service(league::get_recent_results)
            .service(league::get_game_week)
            .service(league::register_team)
            .service(league::get_team_info)
            .service(league::get_all_teams)
            .service(league::update_team)
            .service(league::get_team_history)
            .service(league::add_team_member)
            .service(league::get_team_members)
            .service(league::remove_team_member)
            .service(league::update_team_member)
            .service(league::get_league_users_with_stats)
            .service(league::get_live_scores)
            .service(league::get_game_live_score)
            .service(league::get_active_games)
            .service(league::manage_games)
    );
    // WebSocket routes (authentication handled in route)
    cfg.service(
        web::resource("/game-ws")
            .route(web::get().to(websocket::game_ws_route))
    );
    
    // Admin routes (require admin authentication)
    admin::init_admin_routes(cfg);
}