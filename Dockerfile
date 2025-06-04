FROM rust:1.87 AS builder
WORKDIR /src
COPY . .
RUN cargo build --release
WORKDIR /src/target/release
RUN rm -rf ./build && rm -rf ./deps && rm -rf ./examples && rm -rf ./incremental
WORKDIR /src

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /src/target/release .
EXPOSE 80

CMD [ "/app/travel-agency" ]