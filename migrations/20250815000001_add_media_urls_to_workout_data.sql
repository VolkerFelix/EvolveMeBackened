-- Add image and video URL columns to workout_data table for media attachments
ALTER TABLE workout_data 
ADD COLUMN image_url TEXT,
ADD COLUMN video_url TEXT;

-- Create indexes for media URL lookups
CREATE INDEX IF NOT EXISTS idx_workout_data_image_url ON workout_data(image_url) WHERE image_url IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_workout_data_video_url ON workout_data(video_url) WHERE video_url IS NOT NULL;