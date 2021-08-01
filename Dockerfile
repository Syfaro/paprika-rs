FROM rust:slim AS builder
WORKDIR /build
ENV SQLX_OFFLINE=true
COPY . .
RUN apt-get update && apt-get install -y pkg-config libssl-dev
RUN cargo build --bin paprika-api --release --verbose

FROM debian:10-slim
EXPOSE 8080
RUN apt-get update && apt-get install -y --no-install-recommends openssl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/paprika-api /bin/paprika-api
CMD [ "/bin/paprika-api" ]
