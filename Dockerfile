FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef

# Create and change to the app directory
WORKDIR /app

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libxml2-dev \
    libclang-dev \
    clang \
    && rm -rf /var/lib/apt/lists/*

FROM chef AS planner
COPY . ./
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies (caching Docker layer)
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . ./
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libxml2 \
    libssl3 \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/carmine /app/carmine
CMD ["./carmine"]
