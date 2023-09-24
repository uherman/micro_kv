FROM rust:latest

COPY . .

RUN cargo build --release

ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=3310
EXPOSE 3310

CMD ["./target/release/micro_kv"]