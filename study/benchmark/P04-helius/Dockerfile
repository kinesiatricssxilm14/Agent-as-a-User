# ============================================================
# P04 · Helius-Finance/helius-finance-tracker (RUST)
# ============================================================
FROM rust:1-bookworm AS builder

RUN apt-get update -qq && apt-get install -y \
    git pkg-config libssl-dev libsqlite3-dev \
    2>/dev/null \
 && rm -rf /var/lib/apt/lists/*

ARG SOURCE_SHA=644f1f173bbbbcc47850646a3f58e2aa0ee71764
RUN git clone https://github.com/Helius-Finance/helius-finance-tracker /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
WORKDIR /src
RUN cargo build --release
FROM debian:12-slim
LABEL bench.id="P04" bench.slug="helius" bench.lang="rust"

RUN apt-get update -qq \
 && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libsqlite3-0 sqlite3
RUN rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/helius /usr/local/bin/helius

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data
ENTRYPOINT ["/entrypoint.sh"]
CMD ["helius"]
