-- Add created_at timestamp to apps table
ALTER TABLE apps ADD COLUMN created_at TIMESTAMP DEFAULT NOW();

-- Backfill existing rows with current timestamp
UPDATE apps SET created_at = NOW() WHERE created_at IS NULL;

-- Make column NOT NULL after backfill
ALTER TABLE apps ALTER COLUMN created_at SET NOT NULL;
