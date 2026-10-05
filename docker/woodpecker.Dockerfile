FROM rust:1.99-bookworm AS base

LABEL maintainer="Benedikt Schäfer"
LABEL description="Image for CI with Rust with some prebuilt toolings"
LABEL version="1.3.0"

ENV OPENAPI_GENERATOR_VERSION=7.20.0
ARG OPENAPI_GENERATOR_VERSION=7.20.0
ARG DATABASE_STATE_URL="https://drive.usercontent.google.com/download?id=17lXDQ3A9jHdQbwxm73A0RP-t62SB7gpk&export=download&authuser=0"

RUN rustup component add clippy
RUN rustup component add rustfmt
RUN cargo install sqlx-cli  --no-default-features --features postgres,native-tls --locked
RUN cargo install cargo-audit  --locked
RUN cargo install cargo-machete --locked

RUN apt update && apt install maven jq p7zip-full postgresql-client -y --no-install-recommends
RUN 7z -h
RUN mkdir /oapigen
RUN curl https://raw.githubusercontent.com/OpenAPITools/openapi-generator/master/bin/utils/openapi-generator-cli.sh > /oapigen/openapi-generator-cli 
RUN chmod a+x /oapigen/openapi-generator-cli
RUN /oapigen/openapi-generator-cli version

RUN mkdir /pazufa
RUN curl -o /pazufa/dbstate.sql.7z $DATABASE_STATE_URL 
