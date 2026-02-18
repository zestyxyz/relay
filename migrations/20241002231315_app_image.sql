-- Add image column if it doesn't exist
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'apps' AND column_name = 'image') THEN
        ALTER TABLE apps ADD COLUMN image VARCHAR(1024) NOT NULL DEFAULT '';
    END IF;
END $$;
