-- Fix created_at column to use TIMESTAMPTZ for proper timezone handling (if not already)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'apps'
        AND column_name = 'created_at'
        AND data_type = 'timestamp without time zone'
    ) THEN
        ALTER TABLE apps ALTER COLUMN created_at TYPE TIMESTAMPTZ USING created_at AT TIME ZONE 'UTC';
    END IF;
END $$;
