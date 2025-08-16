use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;
use tracing::{info, debug};

use crate::models::live_game::{
    LiveGame, LivePlayerContribution, LiveScoreEvent,
    LiveGameResponse, LiveGameScoreUpdate
};

#[derive(Debug)]
pub struct LiveGameQueries {
    pool: PgPool,
}

impl LiveGameQueries {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new live game when a regular game starts
    pub async fn create_live_game(
        &self,
        game_id: Uuid,
    ) -> Result<LiveGame, sqlx::Error> {
        info!("Creating live game for game_id: {}", game_id);

        // First get the game details
        let game_query = "
            SELECT 
                g.id,
                g.home_team_id,
                ht.team_name as home_team_name,
                g.away_team_id,
                at.team_name as away_team_name,
                g.scheduled_time,
                g.week_end_date
            FROM league_games g
            JOIN teams ht ON g.home_team_id = ht.id
            JOIN teams at ON g.away_team_id = at.id
            WHERE g.id = $1
        ";

        let game_row = sqlx::query(game_query)
            .bind(game_id)
            .fetch_one(&self.pool)
            .await?;

        let live_game_id = Uuid::new_v4();
        let home_team_id: Uuid = game_row.get("home_team_id");
        let home_team_name: String = game_row.get("home_team_name");
        let away_team_id: Uuid = game_row.get("away_team_id");
        let away_team_name: String = game_row.get("away_team_name");
        let scheduled_time: DateTime<Utc> = game_row.get("scheduled_time");
        let end_time: DateTime<Utc> = game_row.get("week_end_date");

        // Create the live game record
        let live_game = sqlx::query_as!(
            LiveGame,
            r#"
            INSERT INTO live_games (
                id, game_id, home_team_id, home_team_name, away_team_id, away_team_name,
                home_score, away_score, home_power, away_power,
                game_start_time, game_end_time, is_active, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, 0, 0, 0, 0, $7, $8, true, NOW(), NOW())
            RETURNING 
                id, game_id, home_team_id, home_team_name, away_team_id, away_team_name,
                home_score, away_score, home_power, away_power,
                game_start_time, game_end_time, last_score_time, last_scorer_id,
                last_scorer_name, last_scorer_team, is_active, created_at, updated_at
            "#,
            live_game_id,
            game_id,
            home_team_id,
            home_team_name,
            away_team_id,
            away_team_name,
            scheduled_time,
            end_time
        )
        .fetch_one(&self.pool)
        .await?;

        // Initialize player contributions for both teams
        self.initialize_player_contributions(&live_game).await?;

        info!("Successfully created live game {} for game {}", live_game_id, game_id);
        Ok(live_game)
    }

    /// Initialize player contributions for a live game
    async fn initialize_player_contributions(
        &self,
        live_game: &LiveGame,
    ) -> Result<(), sqlx::Error> {
        // Get all team members for both teams
        let members_query = "
            SELECT 
                tm.user_id,
                u.username,
                tm.team_id,
                t.team_name as team_name,
                CASE 
                    WHEN tm.team_id = $1 THEN 'home'
                    WHEN tm.team_id = $2 THEN 'away'
                    ELSE 'unknown'
                END as team_side
            FROM team_members tm
            JOIN users u ON tm.user_id = u.id
            JOIN teams t ON tm.team_id = t.id
            WHERE (tm.team_id = $1 OR tm.team_id = $2)
            AND tm.status = 'active'
        ";

        let members = sqlx::query(members_query)
            .bind(live_game.home_team_id)
            .bind(live_game.away_team_id)
            .fetch_all(&self.pool)
            .await?;

        for member in members {
            let contribution_id = Uuid::new_v4();
            let user_id: Uuid = member.get("user_id");
            let username: String = member.get("username");
            let team_id: Uuid = member.get("team_id");
            let team_name: String = member.get("team_name");
            let team_side: String = member.get("team_side");

            sqlx::query!(
                r#"
                INSERT INTO live_player_contributions (
                    id, live_game_id, user_id, username, team_id, team_name, team_side,
                    current_power, total_score_contribution, contribution_count,
                    is_currently_active, created_at, updated_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, 0, 0, 0, false, NOW(), NOW())
                "#,
                contribution_id,
                live_game.id,
                user_id,
                username,
                team_id,
                team_name,
                team_side
            )
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Get a live game by game_id
    pub async fn get_live_game_by_game_id(
        &self,
        game_id: Uuid,
    ) -> Result<Option<LiveGame>, sqlx::Error> {
        let live_game = sqlx::query_as!(
            LiveGame,
            r#"
            SELECT 
                id, game_id, home_team_id, home_team_name, away_team_id, away_team_name,
                home_score, away_score, home_power, away_power,
                game_start_time, game_end_time, last_score_time, last_scorer_id,
                last_scorer_name, last_scorer_team, is_active, created_at, updated_at
            FROM live_games 
            WHERE game_id = $1 AND is_active = true
            "#,
            game_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(live_game)
    }

    /// Update live game scores when a player contributes
    pub async fn update_live_game_score(
        &self,
        live_game_id: Uuid,
        update: &LiveGameScoreUpdate,
    ) -> Result<LiveGame, sqlx::Error> {
        info!("Updating live game {} score for user {}", live_game_id, update.username);

        // Get current live game state
        let current_game = sqlx::query_as!(
            LiveGame,
            "SELECT * FROM live_games WHERE id = $1",
            live_game_id
        )
        .fetch_one(&self.pool)
        .await?;

        // Determine which team the user belongs to and update accordingly
        let team_side = if update.team_id == current_game.home_team_id {
            "home"
        } else {
            "away"
        };

        let (new_home_score, new_away_score, new_home_power, new_away_power) = if team_side == "home" {
            (
                current_game.home_score + update.score_increase,
                current_game.away_score,
                current_game.home_power + update.power_increase,
                current_game.away_power,
            )
        } else {
            (
                current_game.home_score,
                current_game.away_score + update.score_increase,
                current_game.home_power,
                current_game.away_power + update.power_increase,
            )
        };

        // Update the live game
        let updated_game = sqlx::query_as!(
            LiveGame,
            r#"
            UPDATE live_games 
            SET 
                home_score = $1,
                away_score = $2,
                home_power = $3,
                away_power = $4,
                last_score_time = NOW(),
                last_scorer_id = $5,
                last_scorer_name = $6,
                last_scorer_team = $7,
                updated_at = NOW()
            WHERE id = $8
            RETURNING 
                id, game_id, home_team_id, home_team_name, away_team_id, away_team_name,
                home_score, away_score, home_power, away_power,
                game_start_time, game_end_time, last_score_time, last_scorer_id,
                last_scorer_name, last_scorer_team, is_active, created_at, updated_at
            "#,
            new_home_score,
            new_away_score,
            new_home_power,
            new_away_power,
            update.user_id,
            update.username,
            team_side,
            live_game_id
        )
        .fetch_one(&self.pool)
        .await?;

        // Only update contributions and record events if there's actual score increase
        if update.score_increase > 0 || update.power_increase > 0 {
            // Update player contribution
            self.update_player_contribution(live_game_id, update).await?;

            // Record the score event
            self.record_score_event(live_game_id, update, team_side).await?;
        }

        debug!("Updated live game {}: {} {} - {} {}", 
            live_game_id, 
            updated_game.home_team_name, 
            updated_game.home_score,
            updated_game.away_score,
            updated_game.away_team_name
        );

        Ok(updated_game)
    }

    /// Update a player's contribution in a live game
    async fn update_player_contribution(
        &self,
        live_game_id: Uuid,
        update: &LiveGameScoreUpdate,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE live_player_contributions 
            SET 
                current_power = current_power + $1,
                total_score_contribution = total_score_contribution + $2,
                contribution_count = contribution_count + 1,
                last_contribution_time = NOW(),
                is_currently_active = true,
                updated_at = NOW()
            WHERE live_game_id = $3 AND user_id = $4
            "#,
            update.power_increase,
            update.score_increase,
            live_game_id,
            update.user_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Record a score event
    async fn record_score_event(
        &self,
        live_game_id: Uuid,
        update: &LiveGameScoreUpdate,
        team_side: &str,
    ) -> Result<(), sqlx::Error> {
        let event_id = Uuid::new_v4();
        
        sqlx::query!(
            r#"
            INSERT INTO live_score_events (
                id, live_game_id, user_id, username, team_id, team_side,
                score_points, power_contribution, stamina_gained, strength_gained,
                description, workout_data_id, occurred_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW())
            "#,
            event_id,
            live_game_id,
            update.user_id,
            update.username,
            update.team_id,
            team_side,
            update.score_increase,
            update.power_increase,
            update.stamina_gained,
            update.strength_gained,
            update.description,
            update.workout_data_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get all active live games
    pub async fn get_active_live_games(&self) -> Result<Vec<LiveGame>, sqlx::Error> {
        let games = sqlx::query_as!(
            LiveGame,
            r#"
            SELECT 
                lg.id, lg.game_id, lg.home_team_id, lg.home_team_name, lg.away_team_id, lg.away_team_name,
                lg.home_score, lg.away_score, lg.home_power, lg.away_power,
                lg.game_start_time, lg.game_end_time, lg.last_score_time, lg.last_scorer_id,
                lg.last_scorer_name, lg.last_scorer_team, lg.is_active, lg.created_at, lg.updated_at
            FROM live_games lg
            JOIN league_games g ON lg.game_id = g.id
            WHERE lg.is_active = true 
            AND g.status = 'in_progress'
            AND lg.game_start_time <= NOW() 
            AND lg.game_end_time > NOW()
            ORDER BY lg.game_start_time
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(games)
    }

    /// Get player contributions for a live game
    pub async fn get_live_game_contributions(
        &self,
        live_game_id: Uuid,
    ) -> Result<(Vec<LivePlayerContribution>, Vec<LivePlayerContribution>), sqlx::Error> {
        let contributions = sqlx::query_as!(
            LivePlayerContribution,
            r#"
            SELECT 
                id, live_game_id, user_id, username, team_id, team_name, team_side,
                current_power, total_score_contribution, last_contribution_time,
                contribution_count, is_currently_active, created_at, updated_at
            FROM live_player_contributions 
            WHERE live_game_id = $1
            ORDER BY total_score_contribution DESC
            "#,
            live_game_id
        )
        .fetch_all(&self.pool)
        .await?;

        let mut home_contributions = Vec::new();
        let mut away_contributions = Vec::new();

        for mut contribution in contributions {
            contribution.update_activity_status();
            if contribution.team_side == "home" {
                home_contributions.push(contribution);
            } else {
                away_contributions.push(contribution);
            }
        }

        Ok((home_contributions, away_contributions))
    }

    /// Get recent score events for a live game
    pub async fn get_recent_score_events(
        &self,
        live_game_id: Uuid,
        limit: i32,
    ) -> Result<Vec<LiveScoreEvent>, sqlx::Error> {
        let events = sqlx::query_as!(
            LiveScoreEvent,
            r#"
            SELECT 
                id, live_game_id, user_id, username, team_id, team_side,
                score_points, power_contribution, stamina_gained, strength_gained,
                description, occurred_at
            FROM live_score_events 
            WHERE live_game_id = $1
            ORDER BY occurred_at DESC
            LIMIT $2
            "#,
            live_game_id,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }

    /// Get recent score events with workout details for a live game
    pub async fn get_recent_score_events_with_workout_details(
        &self,
        live_game_id: Uuid,
        limit: i32,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let events = sqlx::query!(
            r#"
            SELECT 
                lse.id, lse.live_game_id, lse.user_id, lse.username, lse.team_id, lse.team_side,
                lse.score_points, lse.power_contribution, lse.stamina_gained, lse.strength_gained,
                lse.description, lse.occurred_at,
                wd.id as "workout_id?",
                wd.created_at as "workout_date?",
                wd.workout_start as "workout_start?",
                wd.workout_end as "workout_end?",
                wd.duration_minutes as "duration_minutes?",
                wd.calories_burned as "calories_burned?",
                wd.avg_heart_rate as "avg_heart_rate?",
                wd.max_heart_rate as "max_heart_rate?",
                wd.min_heart_rate as "min_heart_rate?",
                wd.heart_rate_zones as "heart_rate_zones?",
                wd.stamina_gained as "workout_stamina_gained?",
                wd.strength_gained as "workout_strength_gained?",
                wd.total_points_gained as "total_points_gained?",
                wd.image_url as "image_url?",
                wd.video_url as "video_url?"
            FROM live_score_events lse
            LEFT JOIN workout_data wd ON lse.workout_data_id = wd.id
            WHERE lse.live_game_id = $1
            ORDER BY lse.occurred_at DESC
            LIMIT $2
            "#,
            live_game_id,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        // Convert to JSON objects with workout details
        let events_with_workouts: Vec<serde_json::Value> = events.into_iter().map(|row| {
            let mut event = serde_json::json!({
                "id": row.id,
                "live_game_id": row.live_game_id,
                "user_id": row.user_id,
                "username": row.username,
                "team_id": row.team_id,
                "team_side": row.team_side,
                "score_points": row.score_points,
                "power_contribution": row.power_contribution,
                "stamina_gained": row.stamina_gained,
                "strength_gained": row.strength_gained,
                "description": row.description,
                "occurred_at": row.occurred_at
            });

            // Add workout details if available
            if let Some(workout_id) = row.workout_id {
                event["workout_details"] = serde_json::json!({
                    "id": workout_id.to_string(),
                    "workout_date": row.workout_date,
                    "workout_start": row.workout_start,
                    "workout_end": row.workout_end,
                    "duration_minutes": row.duration_minutes,
                    "calories_burned": row.calories_burned,
                    "avg_heart_rate": row.avg_heart_rate,
                    "max_heart_rate": row.max_heart_rate,
                    "min_heart_rate": row.min_heart_rate,
                    "heart_rate_zones": row.heart_rate_zones,
                    "stamina_gained": row.workout_stamina_gained.unwrap_or(row.stamina_gained),
                    "strength_gained": row.workout_strength_gained.unwrap_or(row.strength_gained),
                    "total_points_gained": row.total_points_gained.unwrap_or(row.score_points),
                    "image_url": row.image_url,
                    "video_url": row.video_url
                });
            }

            event
        }).collect();

        Ok(events_with_workouts)
    }

    /// Finish a live game
    pub async fn finish_live_game(&self, live_game_id: Uuid) -> Result<(), sqlx::Error> {
        // Start a transaction to update both tables atomically
        let mut tx = self.pool.begin().await?;
        
        // Get the game_id from the live_game first
        let live_game = sqlx::query!(
            "SELECT game_id FROM live_games WHERE id = $1",
            live_game_id
        )
        .fetch_one(&mut *tx)
        .await?;
        
        // Update the live_games table
        sqlx::query!(
            "UPDATE live_games SET is_active = false, updated_at = NOW() WHERE id = $1",
            live_game_id
        )
        .execute(&mut *tx)
        .await?;
        
        // Update the corresponding league_games status to 'finished'
        sqlx::query!(
            "UPDATE league_games SET status = 'finished', updated_at = NOW() WHERE id = $1",
            live_game.game_id
        )
        .execute(&mut *tx)
        .await?;
        
        // Commit the transaction
        tx.commit().await?;

        info!("Finished live game: {} and updated league game status", live_game_id);
        Ok(())
    }

    /// Get complete live game response with all data
    pub async fn get_live_game_response(
        &self,
        live_game_id: Uuid,
    ) -> Result<LiveGameResponse, sqlx::Error> {
        let live_game = sqlx::query_as!(
            LiveGame,
            "SELECT * FROM live_games WHERE id = $1",
            live_game_id
        )
        .fetch_one(&self.pool)
        .await?;

        let (home_contributions, away_contributions) = 
            self.get_live_game_contributions(live_game_id).await?;

        let recent_events = self.get_recent_score_events(live_game_id, 10).await?;

        Ok(LiveGameResponse {
            live_game,
            home_contributions,
            away_contributions,
            recent_events,
        })
    }
}