-- Fix created_at column to use TIMESTAMPTZ for proper timezone handling
ALTER TABLE apps ALTER COLUMN created_at TYPE TIMESTAMPTZ USING created_at AT TIME ZONE 'UTC';
