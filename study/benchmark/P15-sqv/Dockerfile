# ============================================================
# P15 · mendrik-private/sqv (RUST)
# ============================================================
FROM rust:1-bookworm AS builder

RUN apt-get update -qq && apt-get install -y \
    git pkg-config libssl-dev libsqlite3-dev \
    2>/dev/null \
 && rm -rf /var/lib/apt/lists/*

ARG SOURCE_SHA=ee7ea40f765690909c050be16796528aa92ba37c
RUN git clone https://github.com/mendrik-private/sqv /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
WORKDIR /src
RUN cargo build --release
FROM debian:12-slim
LABEL bench.id="P15" bench.slug="sqv" bench.lang="rust"

RUN apt-get update -qq \
 && apt-get install -y --no-install-recommends \
    ca-certificates libssl3
RUN apt-get install -y sqlite3 libsqlite3-0 bash 2>/dev/null && rm -rf /var/lib/apt/lists/*

RUN ln -s /usr/local/bin/sqview /usr/local/bin/sqv
COPY --from=builder /src/target/release/sqview /usr/local/bin/sqview

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY seed/bench.db /bench/data/bench.db
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data
ENTRYPOINT ["/entrypoint.sh"]
CMD ["sqview", "/bench/data/bench.db"]
