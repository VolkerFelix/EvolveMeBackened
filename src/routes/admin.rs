use actix_web::web;

use crate::handlers::admin::{
    user_handler,
    team_handler,
    league_handler,
};
use crate::middleware::admin::AdminMiddleware;

pub fn init_admin_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/admin")
            .wrap(AdminMiddleware)
            // User management routes
            .service(
                web::resource("/users")
                    .route(web::get().to(user_handler::get_users))
            )
            .service(
                web::resource("/users/without-team")
                    .route(web::get().to(user_handler::get_users_without_team))
            )
            .service(
                web::resource("/users/{id}")
                    .route(web::get().to(user_handler::get_user_by_id))
            )
            .service(
                web::resource("/users/{id}/status")
                    .route(web::patch().to(user_handler::update_user_status))
            )
            
            // Team management routes
            .service(
                web::resource("/teams")
                    .route(web::get().to(team_handler::get_teams))
                    .route(web::post().to(team_handler::create_team))
            )
            .service(
                web::resource("/teams/{id}")
                    .route(web::get().to(team_handler::get_team_by_id))
                    .route(web::patch().to(team_handler::update_team))
                    .route(web::delete().to(team_handler::delete_team))
            )
            .service(
                web::resource("/teams/{id}/members")
                    .route(web::get().to(team_handler::get_team_members))
                    .route(web::post().to(team_handler::add_team_member))
            )
            .service(
                web::resource("/teams/{team_id}/members/{member_id}")
                    .route(web::patch().to(team_handler::update_team_member))
                    .route(web::delete().to(team_handler::remove_team_member))
            )
            
            // League management routes
            .service(
                web::resource("/leagues")
                    .route(web::get().to(league_handler::get_leagues))
                    .route(web::post().to(league_handler::create_league))
            )
            .service(
                web::resource("/leagues/{id}")
                    .route(web::get().to(league_handler::get_league_by_id))
                    .route(web::patch().to(league_handler::update_league))
            )
            .service(
                web::resource("/leagues/{id}/teams")
                    .route(web::get().to(league_handler::get_league_teams))
                    .route(web::post().to(league_handler::assign_team_to_league))
                    .route(web::delete().to(league_handler::remove_team_from_league))
            )
    );
}