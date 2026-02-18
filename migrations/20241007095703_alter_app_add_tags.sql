-- Add tags column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'apps' AND column_name = 'tags') THEN
        ALTER TABLE apps ADD COLUMN tags VARCHAR(1024) NOT NULL DEFAULT '';
    END IF;
END $$;
