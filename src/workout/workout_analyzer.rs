use std::collections::HashMap;

use crate::models::health_data::{HeartRateData, HeartRateZones, ZoneName};

pub struct WorkoutAnalyzer {
    pub total_duration_min: i32,
    pub zone_durations: HashMap<ZoneName, f32>,
    pub avg_heart_rate: f32,
    pub peak_heart_rate: f32,
    time_above_aerobic_threshold: i32,
    heart_rate_variability: f32,
    zone_changes: i32,
}

impl WorkoutAnalyzer {
    pub fn new(heart_rate: &Vec<HeartRateData>, zones: &HeartRateZones) -> Option<Self> {
        if heart_rate.is_empty() {
            return None;
        }

        let mut analyzer = WorkoutAnalyzer {
            total_duration_min: 0,
            zone_durations: HashMap::new(),
            avg_heart_rate: 0.0,
            peak_heart_rate: 0.0,
            time_above_aerobic_threshold: 0,
            heart_rate_variability: 0.0,
            zone_changes: 0,
        };

        // Sort by timestamp to ensure chronological order
        let mut sorted_data = heart_rate.clone();
        sorted_data.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        let start_time = sorted_data.first().unwrap().timestamp;
        let end_time = sorted_data.last().unwrap().timestamp;
        analyzer.total_duration_min = (end_time - start_time).num_seconds() as i32 / 60; // minutes

        let mut hr_values_for_hrv = Vec::new();
        let mut prev_zone = None;
        let mut hr_sum = 0.0;

        // Process each heart rate data point
        for (index, hr_data) in sorted_data.iter().enumerate() {
            let hr = hr_data.heart_rate;

            // Statistics
            hr_sum += hr;
            analyzer.peak_heart_rate = analyzer.peak_heart_rate.max(hr);
            hr_values_for_hrv.push(hr);

            let zone = zones.get_zone(hr);
            // Calc duration for this sample
            let duration_sec = if index == 0 {
                0.0
            } else {
                (hr_data.timestamp - sorted_data[index - 1].timestamp).num_seconds() as f32
            };

            let duration_min = duration_sec / 60.0;

            // Process zone data
            if let Some(zone_name) = zone {
                *analyzer.zone_durations.entry(zone_name).or_insert(0.0) += duration_min;
                // Count time in aerobic zones
                match zone_name {
                    ZoneName::Zone3 | ZoneName::Zone4 | ZoneName::Zone5 => {
                        analyzer.time_above_aerobic_threshold += duration_min as i32;
                    }
                    _ => {}
                }

                // Count zone changes
                if let Some(prev_zone) = prev_zone {
                    if prev_zone != zone_name {
                        analyzer.zone_changes += 1;
                    }
                }
                prev_zone = Some(zone_name);

            }
        }

        analyzer.avg_heart_rate = hr_sum / sorted_data.len() as f32;
        analyzer.heart_rate_variability = calc_hrv(&hr_values_for_hrv);

        Some(analyzer)
    }
}

fn calc_hrv(hr_values: &Vec<f32>) -> f32 {
    if hr_values.len() < 2 {
        return 0.0;
    }

    let mean = hr_values.iter().sum::<f32>() / hr_values.len() as f32;
    let variance = hr_values.iter()
        .map(|hr| (hr - mean).powi(2))
        .sum::<f32>() / hr_values.len() as f32;
    
    variance.sqrt()
}