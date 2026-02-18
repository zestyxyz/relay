-- Add visible column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'apps' AND column_name = 'visible') THEN
        ALTER TABLE apps ADD COLUMN visible BOOLEAN NOT NULL DEFAULT TRUE;
    END IF;
END $$;
