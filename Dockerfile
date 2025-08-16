# Stage 1: Generate recipe file
FROM rust:1.84 AS chef
RUN cargo install cargo-chef
WORKDIR /app

# Stage 2: Prepare build cache
FROM chef AS planner
# Only copy files needed for dependency resolution
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Build dependencies - this is cached unless dependencies change
FROM chef AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies only
RUN cargo chef cook --release --recipe-path recipe.json

# Stage 4: Build application - this only rebuilds your actual code
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY migrations migrations
COPY .sqlx .sqlx
COPY configuration configuration
COPY scripts scripts
RUN cargo build --release

# Stage 5: Runtime environment
FROM debian:bookworm-slim

RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates postgresql-client \
    # Clean up
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

# Create app directory for config
WORKDIR /app
RUN mkdir -p configuration migrations scripts uploads/workout_media
COPY --from=builder /app/configuration/ /app/configuration/
COPY --from=builder /app/migrations/ /app/migrations/
COPY --from=builder /app/scripts/run_migrations_psql.sh /app/scripts/

# Make migration scripts executable
RUN chmod +x /app/scripts/run_migrations_psql.sh

# Copy the compiled binary from the builder stage
COPY --from=builder /app/target/release/evolveme-backend /usr/local/bin/evolveme-backend

# Expose port
EXPOSE 8080

# Run the application
CMD ["evolveme-backend"]