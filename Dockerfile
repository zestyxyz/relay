# Build stage
FROM rust:latest AS builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Cache bust arg - changes with each commit to force rebuild
ARG CACHEBUST=1

# Copy actual source code
COPY src ./src
COPY migrations ./migrations

# Build the actual application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/relay /app/relay

# Copy frontend and static files
COPY frontend ./frontend
COPY migrations ./migrations

# Create images directory
RUN mkdir -p images

EXPOSE 8000

CMD ["./relay"]
