# Binaries that the deploy pipeline attaches to the latest Codeberg release
ARG RELEASE_ARTIFACTS="elastic-companion pazufa-backend prefixmatch validate-vorgang"

FROM rust:1.99-slim-bookworm AS builder

ARG RUSTFLAGS
ARG SWAGGER_UI_DOWNLOAD_URL
ARG RELEASE_ARTIFACTS

ENV TRACK_PATH="https://codeberg.org/PaZuFa/parlamentszusammenfasser/raw/tag/v0.2.7+v0.1.1/tracks.toml"
ENV SQLX_OFFLINE=true

WORKDIR /app

RUN apt update \
    &&  apt install -y --no-install-recommends git libssl-dev pkg-config libpq5 jq curl ripgrep yq\
    &&  rm -rf /var/lib/apt/lists/*
RUN curl -o tracks.toml $TRACK_PATH
RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid 10001 \
    "pazufa-user"


COPY Cargo.toml Cargo.lock ./
COPY ./.git ./.git
COPY ./.sqlx ./.sqlx
COPY ./migrations ./migrations
COPY ./pazufa-backend-lib ./pazufa-backend-lib
COPY ./track-validation ./track-validation
COPY ./pazufa-backend ./pazufa-backend
COPY ./elastic-companion ./elastic-companion

RUN mkdir /artifacts
RUN nice cargo build --locked --release \
    $(printf '%s\n' $RELEASE_ARTIFACTS | sort -u | sed 's/^/--bin /')
RUN cp tracks.toml /artifacts/
RUN target/release/pazufa-backend --dump-spec | jq -S "." | yq -y > /artifacts/openapi.yml
RUN for bin in $RELEASE_ARTIFACTS; do cp "target/release/$bin" /artifacts/; done

# PAZUFA BACKEND RUNNER
FROM busybox:latest AS pazufa-backend-runner

ENV RUST_LOG=warn,pazufa_backend=info

LABEL maintainer="Benedikt Schäfer"
LABEL description="Backend for the PaZuFa"
# backend-label
LABEL version="0.3.5"

ENV PAZUFA_PORT 8080

COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

RUN mkdir -p /var/log/pazufa-backend && chmod 0766 /var/log/pazufa-backend && chown pazufa-user /var/log/pazufa-backend
RUN mkdir /app && chmod 0766 /app && chown pazufa-user /app

COPY --from=builder /usr/lib/*-linux-gnu/libssl.so.* /usr/lib/
COPY --from=builder /usr/lib/*-linux-gnu/libcrypto.so.* /usr/lib/
COPY --from=builder /usr/lib/*-linux-gnu/libpq.so.* /usr/lib/
COPY --from=builder /usr/lib/*-linux-gnu/libgcc_s.so.* /usr/lib
COPY --from=builder --chmod=0400 --chown=pazufa-user:pazufa-user /artifacts/openapi.yml     /app/openapi.yml
COPY --from=builder --chmod=0400 --chown=pazufa-user:pazufa-user /artifacts/tracks.toml     /app/tracks.toml
COPY --from=builder --chmod=0100 --chown=pazufa-user:pazufa-user /artifacts/pazufa-backend  /app

WORKDIR /app

USER pazufa-user
 
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
            CMD wget -q --spider "http://localhost:$PAZUFA_PORT" || exit 1
ENTRYPOINT ["./pazufa-backend"]

# ELASTIC COMPANION RUNNER
FROM busybox:latest AS elastic-companion-runner

ENV RUST_LOG=warn,elastic_companion=info

LABEL maintainer="Benedikt Schäfer"
LABEL description="Elastic Companion Application to PaZuFa Backend"
# ec-label
LABEL version="0.1.0"

COPY --from=builder /etc/passwd /etc/passwd
COPY --from=builder /etc/group /etc/group

RUN mkdir /app && chmod 0766 /app && chown pazufa-user /app
WORKDIR /app

COPY --from=builder /usr/lib/*-linux-gnu/libssl.so.* /usr/lib/
COPY --from=builder /usr/lib/*-linux-gnu/libcrypto.so.* /usr/lib/
COPY --from=builder /usr/lib/*-linux-gnu/libpq.so.* /usr/lib/
COPY --from=builder /usr/lib/*-linux-gnu/libgcc_s.so.* /usr/lib
COPY --from=builder --chmod=0400 --chown=pazufa-user:pazufa-user /artifacts/tracks.toml         /app/tracks.toml
COPY --from=builder --chmod=0100 --chown=pazufa-user:pazufa-user /artifacts/elastic-companion   /app

USER pazufa-user
 
ENTRYPOINT ["./elastic-companion", "--iv", "--id", "--is"]

# RELEASE ARTIFACTS
# Not meant to be run: exported with `--output type=local` by the deploy pipeline
FROM scratch AS release-artifacts

COPY --from=builder /artifacts/ /
