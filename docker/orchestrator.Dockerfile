FROM rust:1.75 as builder

WORKDIR /app
COPY . .
RUN cargo build --release --bin sim_orchestrator

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/sim_orchestrator /usr/local/bin/

EXPOSE 50051 50052 9091

CMD ["sim_orchestrator"]
