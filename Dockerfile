FROM rust:1.87.0 AS builder

WORKDIR /app

# Copy source code and build
COPY . .
RUN cargo build --release
RUN cargo test --release

FROM rust:1.87.0-slim AS runtime

# Create a non-root user
RUN adduser --disabled-password --gecos "" appuser

# Copy the compiled binary from the builder
COPY --from=builder /app/target/release/elevation-service /usr/local/bin/elevation-service

ENV TILE_SET_CACHE=128
ENV TILE_SET_PATH=/app/data
ENV MAX_POST_SIZE=700kb
ENV RUST_LOG=debug
ENV PORT=3000

USER appuser

EXPOSE 3000

HEALTHCHECK CMD curl --fail http://localhost:3000/status || exit 1

ENTRYPOINT [ "elevation-service" ]
