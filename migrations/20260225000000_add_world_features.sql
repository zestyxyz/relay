-- Add slug for SEO-friendly URLs
ALTER TABLE apps ADD COLUMN IF NOT EXISTS slug VARCHAR(255);
CREATE UNIQUE INDEX IF NOT EXISTS idx_apps_slug ON apps(slug);

-- Add verification for owner editing
ALTER TABLE apps ADD COLUMN IF NOT EXISTS verification_code VARCHAR(64);
ALTER TABLE apps ADD COLUMN IF NOT EXISTS verified_at TIMESTAMPTZ;
