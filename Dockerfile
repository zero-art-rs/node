# syntax=docker/dockerfile:1
FROM rust:1.89-alpine3.20 as builder
# This is important, see https://github.com/rust-lang/docker-rust/issues/85
ENV RUSTFLAGS="-C target-feature=-crt-static"

RUN apk add --no-cache musl-dev openssl-dev build-base git openssh-client protobuf protobuf-dev

WORKDIR /opt

COPY Cargo.lock .
COPY Cargo.toml .

COPY apps/zrt-node/Cargo.toml ./apps/zrt-node/Cargo.toml
COPY crates/api/Cargo.toml ./crates/api/Cargo.toml
COPY crates/callback/Cargo.toml ./crates/callback/Cargo.toml
COPY crates/proof-verifier/Cargo.toml ./crates/proof-verifier/Cargo.toml
COPY crates/storage/Cargo.toml ./crates/storage/Cargo.toml
COPY crates/tests/Cargo.toml ./crates/tests/Cargo.toml
COPY crates/types/Cargo.toml ./crates/types/Cargo.toml


RUN --mount=type=ssh cargo build --release || true

COPY crates crates/
COPY apps apps/

# Build main application with SSH mount for git authentication
RUN --mount=type=ssh cargo build --release -p zrt-node \
	&& mkdir out \
	&& cp target/release/zrt-node out/ \
	&& strip out/zrt-node

FROM alpine:3.20

RUN apk add --no-cache libgcc openssl postgresql-client

COPY --from=builder /opt/out/zrt-node /bin/zrt-node

CMD ["/bin/zrt-node", "run", "--config", "/config.toml"]