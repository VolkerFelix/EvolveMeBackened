use std::fmt::Display;
use uuid::Uuid;
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub enum GameResult {
    Win,
    Loss,
    Draw,
}

impl Display for GameResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone)]
pub struct GameStats {
    pub home_team_score: u32,
    pub away_team_score: u32,
    pub home_team_result: GameResult,
    pub away_team_result: GameResult,
    pub winner_team_id: Option<Uuid>,
}

struct TeamPower {
    pub team_id: Uuid,
    pub total_power: u32,
    pub member_count: u32,
    pub average_power: u32,
}

pub struct GameEvaluator;

impl GameEvaluator {
    pub async fn evaluate_game(pool: &PgPool, home_team_id: &Uuid, away_team_id: &Uuid) -> Result<GameStats, sqlx::Error> {
        tracing::info!("🎮 Evaluating game: {} vs {}", home_team_id, away_team_id);

        let home_team_power = Self::calc_team_power(pool, home_team_id).await?;
        let away_team_power = Self::calc_team_power(pool, away_team_id).await?;

        tracing::info!("Team Powers: Home {} ({}), Away {} ({})",
            home_team_power.team_id, home_team_power.average_power,
            away_team_power.team_id, away_team_power.average_power
        );

        let game_stats = Self::calc_game_outcome(&home_team_power, &away_team_power);

        tracing::info!("Game Stats: Home {} ({}), Away {} ({})",
            game_stats.home_team_score, game_stats.home_team_result,
            game_stats.away_team_score, game_stats.away_team_result
        );

        Ok(game_stats)
    }

    async fn calc_team_power(pool: &PgPool, team_id: &Uuid) -> Result<TeamPower, sqlx::Error> {
        let team_stats = sqlx::query!(
            r#"
            SELECT
                tm.team_id,
                COUNT(tm.user_id) as member_count,
                COALESCE(SUM(ua.stamina + ua.strength), 0) as total_power
            FROM team_members tm
            LEFT JOIN user_avatars ua ON tm.user_id = ua.user_id
            WHERE tm.team_id = $1
            AND tm.status = 'active'
            GROUP BY tm.team_id
            "#,
            team_id
        )
        .fetch_optional(pool)
        .await?;

        match team_stats {
            Some(team_stats) => {
                let total_power = team_stats.total_power.unwrap_or(0) as u32;
                let member_count = team_stats.member_count.unwrap_or(0) as u32;
                let average_power = if member_count > 0 { (total_power / member_count) as u32 } else { 0 };
                Ok(TeamPower {
                    team_id: team_stats.team_id,
                    member_count,
                    total_power,
                    average_power,
                })
            }
            None => {
                Ok(TeamPower {
                    team_id: team_id.clone(),
                    member_count: 0,
                    total_power: 0,
                    average_power: 0,
                })
            }
        }
    }

    fn calc_game_outcome(home_team_power: &TeamPower, away_team_power: &TeamPower) -> GameStats {
        let home_team_score = home_team_power.average_power;
        let away_team_score = away_team_power.average_power;
        let home_team_result;
        let away_team_result;
        let mut winner_team_id = None;

        if home_team_score > away_team_score {
            home_team_result = GameResult::Win;
            away_team_result = GameResult::Loss;
            winner_team_id = Some(home_team_power.team_id);
        } else if home_team_score < away_team_score {
            home_team_result = GameResult::Loss;
            away_team_result = GameResult::Win;
            winner_team_id = Some(away_team_power.team_id);
        } else {
            home_team_result = GameResult::Draw;
            away_team_result = GameResult::Draw;
        }

        GameStats {
            home_team_score,
            away_team_score,
            home_team_result,
            away_team_result,
            winner_team_id,
        }
    }

    pub async fn evaluate_todays_games(pool: &PgPool) -> Result<Vec<(Uuid, GameStats)>, sqlx::Error> {
        // Get all games for today
        let pending_games = sqlx::query!(
            r#"
            SELECT
                lg.id as game_id,
                lg.home_team_id,
                lg.away_team_id,
                lg.scheduled_time
            FROM league_games lg
            WHERE DATE(lg.scheduled_time) = CURRENT_DATE
            AND lg.status = 'scheduled'
            ORDER BY lg.scheduled_time
            "#
        )
        .fetch_all(pool)
        .await?;

        let mut results = Vec::new();

        for game in pending_games {
            tracing::info!("🎯 Evaluating game {} for today", game.game_id);
            let game_stats = Self::evaluate_game(pool, &game.home_team_id, &game.away_team_id).await?;
            results.push((game.game_id, game_stats));
        }

        Ok(results)
    }
}