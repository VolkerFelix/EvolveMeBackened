use reqwest::Client;
use serde_json::json;
use chrono::{Weekday, NaiveTime, Utc, Duration};

mod common;
use common::utils::{spawn_app, make_authenticated_request, get_next_date};
use common::admin_helpers::{create_admin_user_and_login, create_league_season_with_schedule, create_teams_for_test};

#[tokio::test]
async fn test_season_creation_with_dynamic_scheduling() {
    let app = spawn_app().await;
    let client = Client::new();
    
    println!("🎯 Testing Dynamic Season Scheduling");
    
    // Step 1: Create admin user
    let admin_user = create_admin_user_and_login(&app.address).await;
    println!("✅ Created admin user");

    // Step 2: Create a league
    let league_request = json!({
        "name": "Dynamic Scheduling Test League",
        "description": "Testing dynamic season scheduling",
        "max_teams": 4
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
    
    // Step 3: Create teams for the league
    let team_ids = create_teams_for_test(&app.address, &admin_user.token, 4).await;
    
    // Assign teams to league
    for team_id in &team_ids {
        let assign_request = json!({"team_id": team_id});
        let assign_response = make_authenticated_request(
            &client,
            reqwest::Method::POST,
            &format!("{}/admin/leagues/{}/teams", &app.address, league_id),
            &admin_user.token,
            Some(assign_request),
        ).await;
        assert_eq!(assign_response.status(), 201);
    }
    
    println!("✅ Created and assigned 4 teams to league");

    // Step 4: Test default scheduling (Saturday 10 PM UTC)
    let start_date = get_next_date(Weekday::Mon, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
    
    let season_id_default = create_league_season_with_schedule(
        &app.address,
        &admin_user.token,
        league_id,
        "Default Schedule Season",
        &start_date.to_rfc3339(),
        "0 0 22 * * SAT", // Saturday 10 PM UTC
        None, // Use default timezone
        None, // Use default auto_evaluation_enabled
    ).await;
    
    // Fetch season details to verify defaults
    let season_response = make_authenticated_request(
        &client,
        reqwest::Method::GET,
        &format!("{}/admin/leagues/{}/seasons/{}", &app.address, league_id, season_id_default),
        &admin_user.token,
        None,
    ).await;
    
    assert_eq!(season_response.status(), 200);
    let season_data: serde_json::Value = season_response.json().await.unwrap();
    
    // Verify default values (no evaluation_cron since we use every-minute scheduling)
    assert_eq!(season_data["data"]["evaluation_timezone"].as_str().unwrap(), "UTC");
    assert_eq!(season_data["data"]["auto_evaluation_enabled"].as_bool().unwrap(), true);
    assert_eq!(season_data["data"]["game_duration_minutes"].as_i64().unwrap(), 8640); // Default 6 days
    
    println!("✅ Created season with default schedule (every minute, 6-day games)");

    // Step 5: Test custom game duration (30-minute games)
    let custom_season_request = json!({
        "name": "Custom Duration Season",
        "start_date": (start_date + Duration::days(30)).to_rfc3339(),
        "evaluation_timezone": "UTC",
        "auto_evaluation_enabled": true,
        "game_duration_minutes": 30
    });

    let custom_season_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/leagues/{}/seasons", &app.address, league_id),
        &admin_user.token,
        Some(custom_season_request),
    ).await;
    
    assert_eq!(custom_season_response.status(), 201);
    let custom_season_data: serde_json::Value = custom_season_response.json().await.unwrap();
    let season_id_custom = custom_season_data["data"]["id"].as_str().unwrap();
    
    // Verify custom values
    assert_eq!(custom_season_data["data"]["evaluation_timezone"].as_str().unwrap(), "UTC");
    assert_eq!(custom_season_data["data"]["auto_evaluation_enabled"].as_bool().unwrap(), true);
    assert_eq!(custom_season_data["data"]["game_duration_minutes"].as_i64().unwrap(), 30); // 30 minutes
    
    println!("✅ Created season with custom game duration (30-minute games)");

    // Step 6: Test disabled auto-evaluation with 1-day games
    let disabled_season_request = json!({
        "name": "Disabled Auto-Evaluation Season",
        "start_date": (start_date + Duration::days(60)).to_rfc3339(),
        "evaluation_timezone": "Europe/London",
        "auto_evaluation_enabled": false,
        "game_duration_minutes": 1440 // 1 day = 1440 minutes
    });

    let disabled_season_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/leagues/{}/seasons", &app.address, league_id),
        &admin_user.token,
        Some(disabled_season_request),
    ).await;
    
    assert_eq!(disabled_season_response.status(), 201);
    let disabled_season_data: serde_json::Value = disabled_season_response.json().await.unwrap();
    
    // Verify disabled auto-evaluation
    assert_eq!(disabled_season_data["data"]["evaluation_timezone"].as_str().unwrap(), "Europe/London");
    assert_eq!(disabled_season_data["data"]["auto_evaluation_enabled"].as_bool().unwrap(), false);
    assert_eq!(disabled_season_data["data"]["game_duration_minutes"].as_i64().unwrap(), 1440); // 1 day
    
    println!("✅ Created season with disabled auto-evaluation and 1-day games");

    // Step 7: Verify end date is calculated based on team count (4 teams = 6 weeks)
    let expected_end_date = start_date + Duration::weeks(6);
    let actual_end_date = chrono::DateTime::parse_from_rfc3339(
        season_data["data"]["end_date"].as_str().unwrap()
    ).unwrap();
    
    // Check that the dates are on the same day (allowing for time differences)
    assert_eq!(
        actual_end_date.date_naive(),
        expected_end_date.date_naive(),
        "End date should be 6 weeks after start date for 4 teams"
    );
    
    println!("✅ Verified end date calculation (6 weeks for 4 teams)");

    // Step 8: Test invalid game duration (exceeds 30-day limit)
    let invalid_duration_request = json!({
        "name": "Invalid Duration Season",
        "start_date": (start_date + Duration::days(90)).to_rfc3339(),
        "evaluation_timezone": "UTC",
        "auto_evaluation_enabled": true,
        "game_duration_minutes": 50000 // Exceeds 43200 minute (30 day) limit
    });
    
    let invalid_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/leagues/{}/seasons", &app.address, league_id),
        &admin_user.token,
        Some(invalid_duration_request),
    ).await;
    
    // Should reject invalid game duration
    assert_eq!(invalid_response.status(), 400);
    println!("✅ Invalid game duration properly rejected");

    // Step 9: Delete a season and verify scheduler cleanup
    let delete_response = make_authenticated_request(
        &client,
        reqwest::Method::DELETE,
        &format!("{}/admin/leagues/{}/seasons/{}", &app.address, league_id, season_id_custom),
        &admin_user.token,
        None,
    ).await;
    
    assert_eq!(delete_response.status(), 204);
    println!("✅ Deleted season (scheduler cleanup should occur)");

    // Step 10: List all seasons to verify they were created
    let list_response = make_authenticated_request(
        &client,
        reqwest::Method::GET,
        &format!("{}/admin/leagues/{}/seasons", &app.address, league_id),
        &admin_user.token,
        None,
    ).await;
    
    assert_eq!(list_response.status(), 200);
    let list_data: serde_json::Value = list_response.json().await.unwrap();
    let seasons = list_data["data"].as_array().unwrap();
    
    // Should have 2 seasons (deleted one, invalid one was rejected)
    assert_eq!(seasons.len(), 2);
    
    println!("✅ Listed seasons successfully");
    println!("🎉 Dynamic season scheduling test completed successfully!");
}

#[tokio::test]
async fn test_season_scheduling_edge_cases() {
    let app = spawn_app().await;
    let client = Client::new();
    
    println!("🎯 Testing Season Scheduling Edge Cases");
    
    // Create admin and league
    let admin_user = create_admin_user_and_login(&app.address).await;
    
    let league_request = json!({
        "name": "Edge Case Test League",
        "description": "Testing edge cases",
        "max_teams": 2
    });
    
    let league_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/leagues", &app.address),
        &admin_user.token,
        Some(league_request),
    ).await;
    
    let league_data: serde_json::Value = league_response.json().await.unwrap();
    let league_id = league_data["data"]["id"].as_str().unwrap();
    
    // Create minimal teams
    let team_ids = create_teams_for_test(&app.address, &admin_user.token, 2).await;
    for team_id in &team_ids {
        let assign_request = json!({"team_id": team_id});
        make_authenticated_request(
            &client,
            reqwest::Method::POST,
            &format!("{}/admin/leagues/{}/teams", &app.address, league_id),
            &admin_user.token,
            Some(assign_request),
        ).await;
    }

    // Test 1: Very short games (1 hour = 60 minutes)
    let short_game_request = json!({
        "name": "Short Games Season",
        "start_date": get_next_date(Weekday::Wed, NaiveTime::from_hms_opt(10, 0, 0).unwrap()).to_rfc3339(),
        "evaluation_timezone": "UTC",
        "auto_evaluation_enabled": true,
        "game_duration_minutes": 60 // 1 hour games
    });

    let short_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/leagues/{}/seasons", &app.address, league_id),
        &admin_user.token,
        Some(short_game_request),
    ).await;
    
    assert_eq!(short_response.status(), 201);
    let short_season_data: serde_json::Value = short_response.json().await.unwrap();
    let season_id_short = short_season_data["data"]["id"].as_str().unwrap();
    
    println!("✅ Created season with short 1-hour games");

    // Test 2: Different timezone with 7-day games
    let timezone_request = json!({
        "name": "Tokyo Timezone Season",
        "start_date": get_next_date(Weekday::Thu, NaiveTime::from_hms_opt(12, 0, 0).unwrap()).to_rfc3339(),
        "evaluation_timezone": "Asia/Tokyo",
        "auto_evaluation_enabled": true,
        "game_duration_minutes": 10080 // 7 days = 10080 minutes
    });

    let timezone_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/leagues/{}/seasons", &app.address, league_id),
        &admin_user.token,
        Some(timezone_request),
    ).await;
    
    assert_eq!(timezone_response.status(), 201);
    let timezone_season_data: serde_json::Value = timezone_response.json().await.unwrap();
    let season_id_timezone = timezone_season_data["data"]["id"].as_str().unwrap();
    
    println!("✅ Created season with Asia/Tokyo timezone");

    // Test 3: Maximum game duration (30 days = 43200 minutes)
    let max_duration_request = json!({
        "name": "Maximum Duration Season",
        "start_date": get_next_date(Weekday::Fri, NaiveTime::from_hms_opt(15, 0, 0).unwrap()).to_rfc3339(),
        "evaluation_timezone": "UTC",
        "auto_evaluation_enabled": true,
        "game_duration_minutes": 43200 // 30 days = 43200 minutes (maximum allowed)
    });

    let max_duration_response = make_authenticated_request(
        &client,
        reqwest::Method::POST,
        &format!("{}/admin/leagues/{}/seasons", &app.address, league_id),
        &admin_user.token,
        Some(max_duration_request),
    ).await;
    
    assert_eq!(max_duration_response.status(), 201);
    let max_duration_data: serde_json::Value = max_duration_response.json().await.unwrap();
    let season_id_complex = max_duration_data["data"]["id"].as_str().unwrap();
    
    println!("✅ Created season with maximum 30-day game duration");

    // Verify all seasons were created
    let list_response = make_authenticated_request(
        &client,
        reqwest::Method::GET,
        &format!("{}/admin/leagues/{}/seasons", &app.address, league_id),
        &admin_user.token,
        None,
    ).await;
    
    let list_data: serde_json::Value = list_response.json().await.unwrap();
    let seasons = list_data["data"].as_array().unwrap();
    assert_eq!(seasons.len(), 3);
    
    println!("🎉 Season scheduling with game durations test completed successfully!");
}