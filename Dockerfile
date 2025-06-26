# syntax=docker/dockerfile:1
FROM rust:1.85-alpine3.20 as builder
# This is important, see https://github.com/rust-lang/docker-rust/issues/85
ENV RUSTFLAGS="-C target-feature=-crt-static"

RUN apk add --no-cache musl-dev openssl-dev build-base git openssh-client

WORKDIR /opt

COPY Cargo.lock .
COPY Cargo.toml .

COPY crates crates/
COPY apps apps/

# Build main application with SSH mount for git authentication
RUN --mount=type=ssh cargo build --release -p node \
	&& mkdir out \
	&& cp target/release/node out/ \
	&& strip out/node

FROM alpine:3.20

RUN apk add --no-cache libgcc openssl postgresql-client

COPY --from=builder /opt/out/node /bin/node

CMD ["/bin/node", "run", "--config", "/config.toml"]