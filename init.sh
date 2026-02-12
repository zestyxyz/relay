# Wait for postgres to be ready
echo "Waiting for postgres..."
for i in $(seq 1 30); do
    if timeout 1 bash -c "cat < /dev/null > /dev/tcp/postgres/5432" 2>/dev/null; then
        echo "Postgres is ready!"
        break
    fi
    echo "Postgres is unavailable - attempt $i/30, sleeping..."
    sleep 2
done

# Only install sqlx-cli if not already present
if ! command -v sqlx &> /dev/null; then
    echo "Installing sqlx-cli..."
    cargo install sqlx-cli --no-default-features --features postgres
else
    echo "sqlx-cli already installed, skipping..."
fi

# Create database if it doesn't exist (ignore errors if it already exists)
sqlx database create 2>/dev/null || true

# Run migrations
sqlx migrate run

# Build and run
cargo run --release