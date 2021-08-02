FROM rust:slim AS builder
WORKDIR /build
ENV SQLX_OFFLINE=true
COPY . .
RUN cargo build --bin paprika-api --release --verbose

FROM debian:10-slim
EXPOSE 8080
COPY --from=builder /build/target/release/paprika-api /bin/paprika-api
CMD [ "/bin/paprika-api" ]
