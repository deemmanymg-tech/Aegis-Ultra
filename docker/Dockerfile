FROM rust:1.82 as builder
WORKDIR /app
RUN rustup toolchain install nightly && rustup default nightly
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /srv
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/aegis_ultra /usr/local/bin/aegis_ultra
COPY policy/packs/policy.json /srv/policy.json
ENV AEGIS_POLICY_PATH=/srv/policy.json
EXPOSE 8088
CMD ["/usr/local/bin/aegis_ultra"]
