FROM rust:1.75 as builder

WORKDIR /app
COPY . .
RUN cargo build --release --bin link_emulator

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/link_emulator /usr/local/bin/

EXPOSE 60051 60052 9098 9099

CMD ["link_emulator"]
