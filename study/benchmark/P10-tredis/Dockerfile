# ============================================================
# P10 · huseyinbabal/tredis (RUST)
# ============================================================
FROM rust:1-bookworm AS builder

RUN apt-get update -qq && apt-get install -y \
    git pkg-config libssl-dev libsqlite3-dev \
    2>/dev/null \
 && rm -rf /var/lib/apt/lists/*

ARG SOURCE_SHA=d59302c2574eaa80696ce5de8d2dc501221a1ad9
RUN git clone https://github.com/huseyinbabal/tredis /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
WORKDIR /src
RUN cargo build --release
FROM debian:12-slim
LABEL bench.id="P10" bench.slug="tredis" bench.lang="rust"

RUN apt-get update -qq \
 && apt-get install -y --no-install-recommends \
    ca-certificates libssl3
RUN apt-get install -y redis-server 2>/dev/null && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/tredis /usr/local/bin/tredis

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data
ENTRYPOINT ["/entrypoint.sh"]
CMD ["tredis"]
