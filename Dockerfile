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
ENV RUST_LOG=info
ENV PORT=3000

USER appuser

EXPOSE 3000

HEALTHCHECK CMD bash -c 'exec 3<>/dev/tcp/localhost/3000 && echo -e "GET /status HTTP/1.1\r\nHost: localhost:3000\r\nConnection: close\r\n\r\n" >&3 && grep -q "HTTP/1.1 200 OK" <&3'
ENTRYPOINT [ "elevation-service" ]
