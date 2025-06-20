use sqlx::PgPool;
use uuid::Uuid;
use chrono::Utc;
use std::collections::HashMap;

use crate::game::game_evaluator::{GameEvaluator, GameStats};

pub struct GameEvaluationService {
    pool: PgPool,
}

#[derive(Debug)]
pub struct EvaluationResult {
    pub games_evaluated: usize,
    pub games_updated: usize,
    pub errors: Vec<String>,
    pub game_results: HashMap<Uuid, GameStats>,
}

impl GameEvaluationService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Evaluate all scheduled games and update their results in the database
    pub async fn evaluate_and_update_todays_games(&self) -> Result<EvaluationResult, sqlx::Error> {
        tracing::info!("🎯 Starting game evaluation process");
        
        let mut result = EvaluationResult {
            games_evaluated: 0,
            games_updated: 0,
            errors: Vec::new(),
            game_results: HashMap::new(),
        };

        // Get all game results
        let game_evaluations = match GameEvaluator::evaluate_todays_games(&self.pool).await {
            Ok(evaluations) => evaluations,
            Err(e) => {
                let error_msg = format!("Failed to evaluate today's games: {}", e);
                tracing::error!("{}", error_msg);
                result.errors.push(error_msg);
                return Ok(result);
            }
        };

        result.games_evaluated = game_evaluations.len();
        tracing::info!("📊 {} games evaluated", result.games_evaluated);

        // Save the game results
        for (game_id, game_stats) in game_evaluations {
            tracing::info!("🎮 Saving game {} result: {} - {}", 
                game_id, game_stats.home_team_score, game_stats.away_team_score);

            // Update the game result in database
            match self.update_game_result(game_id, &game_stats).await {
                Ok(_) => {
                    result.games_updated += 1;
                    tracing::info!("✅ Successfully updated game {} result", game_id);
                    result.game_results.insert(game_id, game_stats.clone());
                }
                Err(e) => {
                    let error_msg = format!("Failed to save game {} in database: {}", game_id, e);
                    tracing::error!("{}", error_msg);
                    result.errors.push(error_msg);
                }
            }
        }

        tracing::info!("🏁 Game evaluation completed: {}/{} games stored successfully", 
            result.games_updated, result.games_evaluated);

        if !result.errors.is_empty() {
            tracing::warn!("⚠️  {} errors occurred during evaluation", result.errors.len());
            for error in &result.errors {
                tracing::warn!("   - {}", error);
            }
        }

        Ok(result)
    }

    /// Update a specific game's result in the database
    async fn update_game_result(&self, game_id: Uuid, game_stats: &GameStats) -> Result<(), sqlx::Error> {
        // Start a transaction to ensure atomic updates
        let mut tx = self.pool.begin().await?;

        // Update the game result
        sqlx::query!(
            r#"
            UPDATE league_games 
            SET 
                home_score = $2,
                away_score = $3,
                winner_team_id = $4,
                status = 'finished',
                updated_at = $5
            WHERE id = $1
            "#,
            game_id,
            game_stats.home_team_score as i32,
            game_stats.away_team_score as i32,
            game_stats.winner_team_id,
            Utc::now()
        )
        .execute(&mut *tx)
        .await?;

        // Commit the transaction
        tx.commit().await?;

        Ok(())
    }

    /// Evaluate a specific game (for testing or manual evaluation)
    pub async fn evaluate_specific_game(&self, game_id: Uuid) -> Result<GameStats, sqlx::Error> {
        tracing::info!("🎯 Evaluating specific game: {}", game_id);

        // Get the game details
        let game = sqlx::query!(
            r#"
            SELECT home_team_id, away_team_id, status
            FROM league_games 
            WHERE id = $1
            "#,
            game_id
        )
        .fetch_optional(&self.pool)
        .await?;

        match game {
            Some(game_data) => {
                if game_data.status != "scheduled" {
                    return Err(sqlx::Error::RowNotFound);
                }

                let game_stats = GameEvaluator::evaluate_game(
                    &self.pool, 
                    &game_data.home_team_id, 
                    &game_data.away_team_id
                ).await?;

                tracing::info!("✅ Game {} evaluated: {} - {}", 
                    game_id, game_stats.home_team_score, game_stats.away_team_score);

                Ok(game_stats)
            }
            None => {
                tracing::error!("❌ Game {} not found", game_id);
                Err(sqlx::Error::RowNotFound)
            }
        }
    }

    /// Get summary of today's scheduled games
    pub async fn get_todays_game_summary(&self) -> Result<GameSummary, sqlx::Error> {
        let summary = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as total_games,
                COUNT(CASE WHEN status = 'scheduled' THEN 1 END) as scheduled_games,
                COUNT(CASE WHEN status = 'finished' THEN 1 END) as finished_games,
                COUNT(CASE WHEN status = 'postponed' THEN 1 END) as postponed_games
            FROM league_games 
            WHERE DATE(scheduled_time) = CURRENT_DATE
            "#
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(GameSummary {
            total_games: summary.total_games.unwrap_or(0) as usize,
            scheduled_games: summary.scheduled_games.unwrap_or(0) as usize,
            finished_games: summary.finished_games.unwrap_or(0) as usize,
            postponed_games: summary.postponed_games.unwrap_or(0) as usize,
        })
    }
}

#[derive(Debug)]
pub struct GameSummary {
    pub total_games: usize,
    pub scheduled_games: usize,
    pub finished_games: usize,
    pub postponed_games: usize,
}

impl std::fmt::Display for GameSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Today's Games: {} total ({} scheduled, {} finished, {} postponed)", 
            self.total_games, self.scheduled_games, self.finished_games, self.postponed_games)
    }
}

impl std::fmt::Display for EvaluationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Evaluation Result: {}/{} games updated successfully, {} errors", 
            self.games_updated, self.games_evaluated, self.errors.len())
    }
}