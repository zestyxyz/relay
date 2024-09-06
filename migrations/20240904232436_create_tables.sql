CREATE TABLE IF NOT EXISTS relays (
  id SERIAL PRIMARY KEY,
  activitypub_id VARCHAR(255) NOT NULL,
  relay_name VARCHAR(255) NOT NULL,
  inbox VARCHAR(255) NOT NULL,
  outbox VARCHAR(255) NOT NULL,
  public_key VARCHAR(2048) NOT NULL,
  -- exists only for local users
  private_key VARCHAR(4096),
  last_refreshed_at TIMESTAMP,
  is_local boolean NOT NULL
);

CREATE TABLE IF NOT EXISTS apps (
  id SERIAL PRIMARY KEY,
  activitypub_id VARCHAR(255) NOT NULL,
  url VARCHAR(255),
  name VARCHAR(255),
  description VARCHAR(1024),
  is_active boolean NOT NULL
);

CREATE TABLE IF NOT EXISTS activities (
  id SERIAL PRIMARY KEY,
  activitypub_id VARCHAR(255) NOT NULL,
  actor VARCHAR(255) NOT NULL,
  obj VARCHAR(255) NOT NULL,
  kind VARCHAR(255) NOT NULL
);

CREATE TABLE IF NOT EXISTS followers (
  relay_id INT NOT NULL,       -- The user being followed
  follower_id INT NOT NULL,   -- The user who is following
  PRIMARY KEY (relay_id, follower_id),  -- Composite key to ensure unique pairs
  FOREIGN KEY (relay_id) REFERENCES relays(id) ON DELETE CASCADE,
  FOREIGN KEY (follower_id) REFERENCES relays(id) ON DELETE CASCADE
);