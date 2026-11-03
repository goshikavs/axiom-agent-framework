# Production image for goshikq-server. Build from the repository root.
# Never bake development credentials into this image.

FROM rust:1.98-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps/cli ./apps/cli
COPY protocol ./protocol
COPY config ./config
RUN cargo build --release --package goshikq-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /src/target/release/goshikq-server /usr/local/bin/goshikq-server
COPY config /app/config
ENV GOSHIKQ_BIND=0.0.0.0:7400
ENV GOSHIKQ_AUTH_MODE=oidc
ENV GOSHIKQ_LOG_FORMAT=json
EXPOSE 7400
USER 65534
ENTRYPOINT ["goshikq-server"]
