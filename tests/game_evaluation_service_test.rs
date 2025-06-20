use reqwest::Client;
use serde_json::json;
use uuid::Uuid;
use chrono::{Utc, Duration};

mod common;
use common::utils::{spawn_app, create_test_user_and_login, make_authenticated_request};
use common::admin_helpers::{create_admin_user_and_login, create_league_season};
use common::health_data_helpers::{create_elite_health_data, create_advanced_health_data, upload_health_data_for_user};

use evolveme_backend::services::GameEvaluationService;

#[tokio::test]
async fn test_game_evaluation_service_integration() {
    let app = spawn_app().await;
    let client = Client::new();
    
    println!("🎯 Testing Game Evaluation Service Integration");
    
    // Step 1: Set up users with different power levels
    let admin_user = create_admin_user_and_login(&app.address).await;
    let user1 = create_test_user_and_login(&app.address).await; // Elite
    let user2 = create_test_user_and_login(&app.address).await; // Advanced
    let user3 = create_test_user_and_login(&app.address).await; // Elite
    let user4 = create_test_user_and_login(&app.address).await; // Advanced
    
    println!("✅ Created 4 users + 1 admin");

    // Step 2: Upload health data to create power differences
    upload_health_data_for_user(&client, &app.address, &user1.token, create_elite_health_data()).await.unwrap();
    upload_health_data_for_user(&client, &app.address, &user2.token, create_advanced_health_data()).await.unwrap();
    upload_health_data_for_user(&client, &app.address, &user3.token, create_elite_health_data()).await.unwrap();
    upload_health_data_for_user(&client, &app.address, &user4.token, create_advanced_health_data()).await.unwrap();
    
    println!("✅ Uploaded health data for all users");

    // Step 3: Create league and teams
    let league_request = json!({
        "name": "Game Evaluation Test League",
        "description": "Testing game evaluation service",
        "max_teams": 2
    });
    
    let league_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/leagues", &app.address),
        &admin_user.token,
        Some(league_request),
    ).await;
    
    assert_eq!(league_response.status(), 201);
    let league_data: serde_json::Value = league_response.json().await.unwrap();
    let league_id = league_data["data"]["id"].as_str().unwrap();
    
    // Create teams
    let team1_request = json!({
        "name": "Power Team",
        "color": "#FF0000",
        "owner_id": user1.user_id
    });
    
    let team1_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/teams", &app.address),
        &admin_user.token,
        Some(team1_request)
    ).await;
    
    assert_eq!(team1_response.status(), 201);
    let team1_data: serde_json::Value = team1_response.json().await.unwrap();
    let team1_id = team1_data["data"]["id"].as_str().unwrap();
    
    let team2_request = json!({
        "name": "Weaker Team",
        "color": "#0000FF",
        "owner_id": user3.user_id
    });
    
    let team2_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/teams", &app.address),
        &admin_user.token,
        Some(team2_request)
    ).await;
    
    assert_eq!(team2_response.status(), 201);
    let team2_data: serde_json::Value = team2_response.json().await.unwrap();
    let team2_id = team2_data["data"]["id"].as_str().unwrap();
    
    // Add members to teams
    let add_member1_request = json!({
        "user_id": user2.user_id,
        "role": "member"
    });
    
    let member1_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/teams/{}/members", &app.address, team1_id),
        &admin_user.token,
        Some(add_member1_request),
    ).await;
    assert_eq!(member1_response.status(), 201);
    
    let add_member2_request = json!({
        "user_id": user4.user_id,
        "role": "member"
    });
    
    let member2_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/teams/{}/members", &app.address, team2_id),
        &admin_user.token,
        Some(add_member2_request),
    ).await;
    assert_eq!(member2_response.status(), 201);
    
    // Assign teams to league
    let assign_team1_request = json!({"team_id": team1_id});
    let assign1_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/leagues/{}/teams", &app.address, league_id),
        &admin_user.token,
        Some(assign_team1_request),
    ).await;
    assert_eq!(assign1_response.status(), 201);
    
    let assign_team2_request = json!({"team_id": team2_id});
    let assign2_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/leagues/{}/teams", &app.address, league_id),
        &admin_user.token,
        Some(assign_team2_request),
    ).await;
    assert_eq!(assign2_response.status(), 201);
    
    println!("✅ Created teams and assigned to league");

    // Step 4: Create a season with games for today
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let start_date = format!("{}T22:00:00Z", today);
    
    let season_id = create_league_season(
        &app.address,
        &admin_user.token,
        league_id,
        "Game Evaluation Test Season",
        &start_date
    ).await;
    
    println!("✅ Created season with games for today");

    // Step 5: Test the GameEvaluationService
    let evaluation_service = GameEvaluationService::new(app.db_pool.clone());
    
    // Get today's game summary before evaluation
    let summary_before = evaluation_service.get_todays_game_summary().await.unwrap();
    println!("📊 Before evaluation: {}", summary_before);
    assert!(summary_before.scheduled_games > 0, "Should have scheduled games for today");
    
    // Evaluate today's games
    let evaluation_result = evaluation_service.evaluate_and_update_todays_games().await.unwrap();
    println!("🎮 Evaluation result: {}", evaluation_result);
    
    // Verify evaluation results
    assert!(evaluation_result.games_evaluated > 0, "Should have evaluated at least one game");
    assert_eq!(evaluation_result.games_updated, evaluation_result.games_evaluated, "All games should be updated successfully");
    assert!(evaluation_result.errors.is_empty(), "Should have no errors");
    
    // Get today's game summary after evaluation
    let summary_after = evaluation_service.get_todays_game_summary().await.unwrap();
    println!("📊 After evaluation: {}", summary_after);
    assert_eq!(summary_after.scheduled_games, 0, "Should have no scheduled games left");
    assert!(summary_after.finished_games > 0, "Should have finished games");
    
    // Verify game results make sense
    for (game_id, game_stats) in evaluation_result.game_results {
        println!("🏆 Game {}: {} - {} (Winner: {:?})", 
            game_id, game_stats.home_team_score, game_stats.away_team_score, game_stats.winner_team_id);
        
        // Verify game stats are reasonable
        assert!(game_stats.home_team_score >= 0, "Home score should be non-negative");
        assert!(game_stats.away_team_score >= 0, "Away score should be non-negative");
        
        // If there's a winner, verify it matches the scores
        if let Some(winner_id) = game_stats.winner_team_id {
            assert!(
                game_stats.home_team_score != game_stats.away_team_score,
                "If there's a winner, scores should not be equal"
            );
        } else {
            assert_eq!(
                game_stats.home_team_score, game_stats.away_team_score,
                "If no winner, scores should be equal (draw)"
            );
        }
    }
    
    println!("✅ Game Evaluation Service integration test completed successfully");
}