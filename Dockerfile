FROM rust:1.95-slim-bookworm AS builder

ARG RUSTFLAGS


ENV TRACK_PATH="https://codeberg.org/PaZuFa/parlamentszusammenfasser/raw/tag/v0.2.3+0.0.5/docs/specs/tracks.toml"

WORKDIR /app

RUN apt update \
    &&  apt install -y --no-install-recommends git libssl-dev pkg-config libpq5 jq curl ripgrep\
    &&  rm -rf /var/lib/apt/lists/*

RUN curl -o tracks.toml $TRACK_PATH

RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid 10001 \
    "pazufa-backend"


COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY ./.git ./.git
COPY ./.sqlx ./.sqlx
COPY ./migrations ./migrations
RUN curl -o ./tracks.toml $TRACK_PATH
COPY ./track-validation ./track-validation
COPY ./src ./src

ENV SQLX_OFFLINE=true

RUN nice cargo build --release --locked

FROM busybox:latest AS runner

LABEL maintainer="Benedikt Schäfer"
LABEL description="Backend for the PaZuFa"
LABEL version="0.2.10"

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
            CMD wget -q --spider "http://localhost:80" || exit 1


COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

COPY --from=builder /usr/lib/*-linux-gnu/libssl.so.* /usr/lib/
COPY --from=builder /usr/lib/*-linux-gnu/libcrypto.so.* /usr/lib/
COPY --from=builder /usr/lib/*-linux-gnu/libpq.so.* /usr/lib/
COPY --from=builder /usr/lib/*-linux-gnu/libgcc_s.so.* /usr/lib

RUN mkdir -p /var/log/pazufa-backend && chmod 0766 /var/log/pazufa-backend && chown pazufa-backend /var/log/pazufa-backend

RUN mkdir /app && chmod 0766 /app && chown pazufa-backend /app

COPY --from=builder --chmod=0400 --chown=pazufa-backend:pazufa-backend /app/tracks.toml /app/tracks.toml
COPY --from=builder --chmod=0100 --chown=pazufa-backend:pazufa-backend /app/target/release/pazufa-backend /app/pazufa-backend

WORKDIR /app

USER pazufa-backend
ENV RUST_LOG=warn,pazufa_backend=info
 
ENTRYPOINT ["./pazufa-backend"]
