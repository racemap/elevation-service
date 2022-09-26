FROM rust:1-buster as builder

COPY . /app
WORKDIR /app
RUN cargo build --release --bins --tests

ENV PORT 3000
ENV TILE_SET_CACHE 128
ENV TILE_SET_PATH /app/data

EXPOSE 3000

HEALTHCHECK CMD curl --fail http://localhost:3000/health || exit 1

CMD ["/app/target/release/elevation-service"]
