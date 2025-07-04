-- Leagues (the main league entity)
CREATE TABLE IF NOT EXISTS leagues (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    max_teams INTEGER NOT NULL DEFAULT 16,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- League seasons (multiple seasons per league)
CREATE TABLE IF NOT EXISTS league_seasons (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    league_id UUID NOT NULL REFERENCES leagues(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    start_date TIMESTAMPTZ NOT NULL,
    end_date TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- League memberships (which teams belong to which leagues)
CREATE TABLE IF NOT EXISTS league_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    league_id UUID NOT NULL REFERENCES leagues(id) ON DELETE CASCADE,
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status VARCHAR(50) NOT NULL DEFAULT 'active', -- active, inactive, banned
    
    UNIQUE(league_id, team_id)
);

-- League teams (which teams are in which season)
CREATE TABLE IF NOT EXISTS league_teams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    season_id UUID NOT NULL REFERENCES league_seasons(id) ON DELETE CASCADE,
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    UNIQUE(season_id, team_id)
);

-- Create index for league teams
CREATE INDEX IF NOT EXISTS idx_league_teams_season ON league_teams(season_id);
CREATE INDEX IF NOT EXISTS idx_league_teams_team ON league_teams(team_id);

-- League games/matches
CREATE TABLE IF NOT EXISTS league_games (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    season_id UUID NOT NULL REFERENCES league_seasons(id) ON DELETE CASCADE,
    home_team_id UUID NOT NULL REFERENCES teams(id),
    away_team_id UUID NOT NULL REFERENCES teams(id),
    scheduled_time TIMESTAMPTZ NOT NULL, -- Always Saturday 10pm
    week_number INTEGER NOT NULL,
    is_first_leg BOOLEAN NOT NULL DEFAULT TRUE, -- True for first meeting, false for second
    status VARCHAR(50) NOT NULL DEFAULT 'scheduled', -- scheduled, live, finished, postponed
    winner_team_id UUID NULL,
    home_score INTEGER,
    away_score INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT valid_status CHECK (status IN ('scheduled', 'live', 'finished', 'postponed')),
    CONSTRAINT different_teams CHECK (home_team_id != away_team_id)
);

-- League standings
CREATE TABLE IF NOT EXISTS league_standings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    season_id UUID NOT NULL REFERENCES league_seasons(id) ON DELETE CASCADE,
    team_id UUID NOT NULL REFERENCES teams(id),
    games_played INTEGER NOT NULL DEFAULT 0,
    wins INTEGER NOT NULL DEFAULT 0,
    draws INTEGER NOT NULL DEFAULT 0,
    losses INTEGER NOT NULL DEFAULT 0,
    points INTEGER GENERATED ALWAYS AS (wins * 3 + draws) STORED,
    position INTEGER NOT NULL DEFAULT 1,
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    UNIQUE(season_id, team_id)
);

-- Create indexes for performance
CREATE INDEX IF NOT EXISTS idx_league_games_season_id ON league_games(season_id);
CREATE INDEX IF NOT EXISTS idx_league_games_scheduled_time ON league_games(scheduled_time);
CREATE INDEX IF NOT EXISTS idx_league_games_status ON league_games(status);
CREATE INDEX IF NOT EXISTS idx_league_games_teams ON league_games(home_team_id, away_team_id);
CREATE INDEX IF NOT EXISTS idx_league_standings_season ON league_standings(season_id, points DESC);

-- Create trigger functions for validation
CREATE OR REPLACE FUNCTION validate_league_game_teams()
RETURNS TRIGGER AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM league_teams lt 
        WHERE lt.season_id = NEW.season_id 
        AND lt.team_id = NEW.home_team_id
    ) OR NOT EXISTS (
        SELECT 1 FROM league_teams lt 
        WHERE lt.season_id = NEW.season_id 
        AND lt.team_id = NEW.away_team_id
    ) THEN
        RAISE EXCEPTION 'Both teams must be in the league season';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION validate_league_standing_team()
RETURNS TRIGGER AS $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM league_teams lt 
        WHERE lt.season_id = NEW.season_id 
        AND lt.team_id = NEW.team_id
    ) THEN
        RAISE EXCEPTION 'Team must be in the league season';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create triggers
CREATE TRIGGER validate_league_game_teams_trigger
    BEFORE INSERT OR UPDATE ON league_games
    FOR EACH ROW
    EXECUTE FUNCTION validate_league_game_teams();

CREATE TRIGGER validate_league_standing_team_trigger
    BEFORE INSERT OR UPDATE ON league_standings
    FOR EACH ROW
    EXECUTE FUNCTION validate_league_standing_team();