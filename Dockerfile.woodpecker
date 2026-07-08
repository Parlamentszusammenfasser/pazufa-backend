FROM rust:1.96-bookworm AS base

LABEL maintainer="Benedikt Schäfer"
LABEL description="Image for CI with Rust with some prebuilt toolings"
LABEL version="1.1.0"

ENV OPENAPI_GENERATOR_VERSION=7.20.0
ARG OPENAPI_GENERATOR_VERSION=7.20.0

RUN rustup component add clippy
RUN rustup component add rustfmt
RUN cargo install sqlx-cli  --no-default-features --features postgres,native-tls --locked
RUN cargo install cargo-audit  --locked
RUN cargo install cargo-machete --locked

RUN apt update && apt install maven jq -y --no-install-recommends
RUN mkdir /oapigen
RUN curl https://raw.githubusercontent.com/OpenAPITools/openapi-generator/master/bin/utils/openapi-generator-cli.sh > /oapigen/openapi-generator-cli 
RUN chmod a+x /oapigen/openapi-generator-cli
