use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, FromRow, Serialize)]
pub struct WorkoutData {
    pub id: Uuid,
    pub user_id: Uuid,
    pub device_id: String,
    pub timestamp: DateTime<Utc>,
    pub heart_rate: Option<Vec<HeartRateData>>,
    pub calories_burned: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub image_url: Option<String>,
    pub video_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow, sqlx::Decode)]
pub struct HeartRateData {
    pub timestamp: DateTime<Utc>,
    pub heart_rate: i32,
}

#[derive(Debug, Deserialize)]
pub struct WorkoutDataSyncRequest {
    pub device_id: String,
    pub timestamp: DateTime<Utc>,
    pub heart_rate: Option<Vec<HeartRateData>>,
    pub calories_burned: Option<i32>,
    pub workout_uuid: String, // Required: Apple Health workout UUID for duplicate prevention
    pub workout_start: Option<DateTime<Utc>>, // Actual workout start time
    pub workout_end: Option<DateTime<Utc>>, // Actual workout end time
    pub image_url: Option<String>,
    pub video_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkoutDataSyncData {
    pub sync_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UserProfile {
    pub age: i32,
    pub gender: Gender,
    pub resting_heart_rate: Option<i32>,
    pub max_heart_rate: Option<i32>,
    pub stored_heart_rate_zones: Option<HeartRateZones>,
}

#[derive(Debug, Clone)]
pub enum Gender {
    Male,
    Female,
    Other, // Use male formulas as default
}

#[derive(Debug, Clone)]
pub struct ZoneRange {
    pub low: i32,
    pub high: i32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ZoneName {
    Zone1,
    Zone2,
    Zone3,
    Zone4,
    Zone5,
}

#[derive(Debug, Clone)]
pub struct HeartRateZones {
    pub zones: HashMap<ZoneName, ZoneRange>,
}

impl HeartRateZones {
    pub fn new(hhr: i32, resting_heart_rate: i32, max_heart_rate: i32) -> Self {
        let zone_1 = ZoneRange {
            low: 0, // Zone 1 starts from 0 bpm to capture all heart rates including below resting
            high: resting_heart_rate + (hhr as f32 * 0.6) as i32 - 1,
        };
        let zone_2 = ZoneRange {
            low: resting_heart_rate + (hhr as f32 * 0.6) as i32,
            high: resting_heart_rate + (hhr as f32 * 0.7) as i32 - 1,
        };
        let zone_3 = ZoneRange {
            low: resting_heart_rate + (hhr as f32 * 0.7) as i32,
            high: resting_heart_rate + (hhr as f32 * 0.8) as i32 - 1,
        };
        let zone_4 = ZoneRange {
            low: resting_heart_rate + (hhr as f32 * 0.8) as i32,
            high: resting_heart_rate + (hhr as f32 * 0.9) as i32 - 1,
        };
        let zone_5 = ZoneRange {
            low: resting_heart_rate + (hhr as f32 * 0.9) as i32,
            high: max_heart_rate,
        };
        Self {
            zones: HashMap::from([
                (ZoneName::Zone1, zone_1),
                (ZoneName::Zone2, zone_2),
                (ZoneName::Zone3, zone_3),
                (ZoneName::Zone4, zone_4),
                (ZoneName::Zone5, zone_5),
            ]),
        }
    }

    pub fn from_stored_zones(
        resting_heart_rate: i32,
        zone_1_max: i32,
        zone_2_max: i32,
        zone_3_max: i32,
        zone_4_max: i32,
        zone_5_max: i32,
    ) -> Self {
        let zone_1 = ZoneRange {
            low: 0, // Zone 1 starts from 0 bpm to capture all heart rates
            high: zone_1_max,
        };
        let zone_2 = ZoneRange {
            low: zone_1_max + 1,
            high: zone_2_max,
        };
        let zone_3 = ZoneRange {
            low: zone_2_max + 1,
            high: zone_3_max,
        };
        let zone_4 = ZoneRange {
            low: zone_3_max + 1,
            high: zone_4_max,
        };
        let zone_5 = ZoneRange {
            low: zone_4_max + 1,
            high: zone_5_max,
        };
        Self {
            zones: HashMap::from([
                (ZoneName::Zone1, zone_1),
                (ZoneName::Zone2, zone_2),
                (ZoneName::Zone3, zone_3),
                (ZoneName::Zone4, zone_4),
                (ZoneName::Zone5, zone_5),
            ]),
        }
    }

    pub fn get_zone(&self, heart_rate: f32) -> Option<ZoneName> {
        for (zone_name, zone_range) in &self.zones {
            if heart_rate >= zone_range.low as f32 && heart_rate <= zone_range.high as f32 {
                return Some(*zone_name);
            }
        }
        None
    }
}

#[derive(serde::Serialize)]
pub struct ActivitySummaryResponse {
    pub recent_workouts: i32,
    pub total_sessions: i32,
    pub zone_distribution: HashMap<String, f32>,
    pub last_sync: Option<DateTime<Utc>>,
    pub weekly_stats: WeeklyStats,
    pub monthly_trend: MonthlyTrend,
}

#[derive(serde::Serialize)]
pub struct WeeklyStats {
    pub total_calories: i32,
    pub total_exercise_time: i32, // in minutes
    pub strength_sessions: i32,
    pub cardio_sessions: i32,
}

#[derive(serde::Serialize)]
pub struct MonthlyTrend {
    pub stamina_gain: i32,
    pub strength_gain: i32,
}