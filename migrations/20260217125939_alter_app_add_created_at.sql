-- Add created_at timestamp to apps table if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'apps' AND column_name = 'created_at') THEN
        ALTER TABLE apps ADD COLUMN created_at TIMESTAMPTZ DEFAULT NOW();
        UPDATE apps SET created_at = NOW() WHERE created_at IS NULL;
        ALTER TABLE apps ALTER COLUMN created_at SET NOT NULL;
    END IF;
END $$;
