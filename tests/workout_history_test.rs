use reqwest::Client;
use serde_json::json;
use uuid::Uuid;
use chrono::{DateTime, Utc};

mod common;
use common::utils::spawn_app;

use crate::common::{
    health_data_helpers::{
        create_advanced_health_data,
        create_elite_health_data,
    },
    utils::create_test_user_and_login,
};

#[tokio::test]
async fn test_workout_history_empty() {
    // Set up the test app
    let test_app = spawn_app().await;
    let client = Client::new();

    let test_user = create_test_user_and_login(&test_app.address).await;

    // Test workout history with no data
    let history_response = client
        .get(&format!("{}/health/history", &test_app.address))
        .header("Authorization", format!("Bearer {}", test_user.token))
        .send()
        .await
        .expect("Failed to execute workout history request.");

    assert!(history_response.status().is_success(), "Workout history should succeed");

    let history_body: serde_json::Value = history_response
        .json()
        .await
        .expect("Failed to parse workout history response");

    assert_eq!(history_body["success"], true);
    assert_eq!(history_body["data"]["workouts"].as_array().unwrap().len(), 0);
    assert_eq!(history_body["data"]["pagination"]["total"], 0);
}

#[tokio::test]
async fn test_workout_history_with_data() {
    // Set up the test app
    let test_app = spawn_app().await;
    let client = Client::new();

    let test_user = create_test_user_and_login(&test_app.address).await;
    let token = test_user.token;

    let health_data = create_advanced_health_data();

    let health_response = client
        .post(&format!("{}/health/upload_health", &test_app.address))
        .header("Authorization", format!("Bearer {}", token))
        .json(&health_data)
        .send()
        .await
        .expect("Failed to execute health upload request.");

    assert!(health_response.status().is_success(), "Health upload should succeed");

    // Wait a bit for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Test workout history with data
    let history_response = client
        .get(&format!("{}/health/history?limit=10", &test_app.address))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to execute workout history request.");

    assert!(history_response.status().is_success(), "Workout history should succeed");

    let history_body: serde_json::Value = history_response
        .json()
        .await
        .expect("Failed to parse workout history response");

    assert_eq!(history_body["success"], true);
    
    let workouts = history_body["data"]["workouts"].as_array().unwrap();
    assert_eq!(workouts.len(), 1, "Should have one workout");

    let workout = &workouts[0];
    assert!(workout["id"].as_str().is_some());
    assert!(workout["calories_burned"].as_f64().unwrap() > 0.0);
    assert!(workout["avg_heart_rate"].as_f64().unwrap() > 0.0);
    assert!(workout["max_heart_rate"].as_f64().unwrap() > 0.0);
    assert!(workout["duration_minutes"].as_i64().unwrap() > 0);
    
    // Check that stat gains are recorded (might be 0 depending on the workout)
    assert!(workout["stamina_gained"].as_i64().unwrap() >= 0);
    assert!(workout["strength_gained"].as_i64().unwrap() >= 0);

    // Check pagination
    let pagination = &history_body["data"]["pagination"];
    assert_eq!(pagination["total"], 1);
    assert_eq!(pagination["limit"], 10);
    assert_eq!(pagination["offset"], 0);
    assert_eq!(pagination["has_more"], false);
}

#[tokio::test]
async fn test_workout_history_pagination() {
    // Set up the test app
    let test_app = spawn_app().await;
    let client = Client::new();

    let test_user = create_test_user_and_login(&test_app.address).await;
    let token = test_user.token;

    // Upload multiple health data entries
    for _ in 0..5 {
        let health_data = create_advanced_health_data();

        let health_response = client
            .post(&format!("{}/health/upload_health", &test_app.address))
            .header("Authorization", format!("Bearer {}", token))
            .json(&health_data)
            .send()
            .await
            .expect("Failed to execute health upload request.");

        assert!(health_response.status().is_success(), "Health upload should succeed");
    }

    // Wait for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Test pagination with limit=2
    let history_response = client
        .get(&format!("{}/health/history?limit=2&offset=0", &test_app.address))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to execute workout history request.");

    assert!(history_response.status().is_success(), "Workout history should succeed");

    let history_body: serde_json::Value = history_response
        .json()
        .await
        .expect("Failed to parse workout history response");

    assert_eq!(history_body["success"], true);
    
    let workouts = history_body["data"]["workouts"].as_array().unwrap();
    assert_eq!(workouts.len(), 2, "Should have 2 workouts with limit=2");

    let pagination = &history_body["data"]["pagination"];
    assert_eq!(pagination["total"], 5);
    assert_eq!(pagination["limit"], 2);
    assert_eq!(pagination["offset"], 0);
    assert_eq!(pagination["has_more"], true);

    // Test second page
    let history_response_2 = client
        .get(&format!("{}/health/history?limit=2&offset=2", &test_app.address))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to execute workout history request.");

    assert!(history_response_2.status().is_success(), "Workout history page 2 should succeed");

    let history_body_2: serde_json::Value = history_response_2
        .json()
        .await
        .expect("Failed to parse workout history response");

    let workouts_2 = history_body_2["data"]["workouts"].as_array().unwrap();
    assert_eq!(workouts_2.len(), 2, "Should have 2 workouts on page 2");

    let pagination_2 = &history_body_2["data"]["pagination"];
    assert_eq!(pagination_2["offset"], 2);
    assert_eq!(pagination_2["has_more"], true);
}

#[tokio::test]
async fn test_workout_history_unauthorized() {
    // Set up the test app
    let test_app = spawn_app().await;
    let client = Client::new();

    // Test workout history without authentication
    let history_response = client
        .get(&format!("{}/health/history", &test_app.address))
        .send()
        .await
        .expect("Failed to execute workout history request.");

    assert!(history_response.status().is_client_error(), "Should fail without auth");
    assert_eq!(history_response.status().as_u16(), 401);
}

#[tokio::test]
async fn test_workout_history_with_stats() {
    // Set up the test app
    let test_app = spawn_app().await;
    let client = Client::new();

    let test_user = create_test_user_and_login(&test_app.address).await;
    let token = test_user.token;

    // Upload health data with high intensity to generate stats
    let health_data = create_elite_health_data();

    let health_response = client
        .post(&format!("{}/health/upload_health", &test_app.address))
        .header("Authorization", format!("Bearer {}", token))
        .json(&health_data)
        .send()
        .await
        .expect("Failed to execute health upload request.");

    assert!(health_response.status().is_success(), "Health upload should succeed");

    // Wait for processing
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Test workout history
    let history_response = client
        .get(&format!("{}/health/history", &test_app.address))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .expect("Failed to execute workout history request.");

    assert!(history_response.status().is_success(), "Workout history should succeed");

    let history_body: serde_json::Value = history_response
        .json()
        .await
        .expect("Failed to parse workout history response");

    assert_eq!(history_body["success"], true);
    
    let workouts = history_body["data"]["workouts"].as_array().unwrap();
    assert_eq!(workouts.len(), 1, "Should have one workout");

    let workout = &workouts[0];
    
    // Verify workout details
    assert!(workout["duration_minutes"].as_i64().unwrap() > 0);
    assert!(workout["calories_burned"].as_f64().unwrap() > 0.0);
    
    // Verify heart rate calculations
    let avg_hr = workout["avg_heart_rate"].as_f64().unwrap();
    let max_hr = workout["max_heart_rate"].as_f64().unwrap();
    assert!(avg_hr > 0.0 && avg_hr < 200.0, "Average HR should be reasonable");
    assert!(max_hr > 0.0 && max_hr < 250.0, "Max HR should be reasonable");

    // Check total stats
    let stamina = workout["stamina_gained"].as_i64().unwrap();
    let strength = workout["strength_gained"].as_i64().unwrap();
    assert!(stamina > 0, "Stamina should be positive");
    assert!(strength > 0, "Strength should be positive");
}