-- Add is_adult column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'apps' AND column_name = 'is_adult') THEN
        ALTER TABLE apps ADD COLUMN is_adult BOOLEAN NOT NULL DEFAULT FALSE;
    END IF;
END $$;
