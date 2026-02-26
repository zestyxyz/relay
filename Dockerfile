# Build stage - use bookworm to match runtime glibc version
FROM rust:bookworm AS builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

# Remove the dummy binary and source (keep compiled dependencies)
RUN rm -rf src && rm -f target/release/relay target/release/deps/relay*

# Copy actual source code
COPY src ./src
COPY migrations ./migrations

# Build the actual application (dependencies are cached, only app code compiles)
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
