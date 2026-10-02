# ============================================================
# P03 · elio-fm/elio (RUST)
# ============================================================
FROM rust:1-bookworm AS builder

RUN apt-get update -qq && apt-get install -y \
    git pkg-config libssl-dev libsqlite3-dev \
    2>/dev/null \
 && rm -rf /var/lib/apt/lists/*

ARG SOURCE_SHA=8f0eae84143742e287fdf6954dbfa1eec8f8e935
RUN git clone https://github.com/elio-fm/elio /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
WORKDIR /src
RUN cargo build --release
FROM debian:12-slim
LABEL bench.id="P03" bench.slug="elio" bench.lang="rust"

RUN apt-get update -qq \
 && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 xdg-utils
RUN rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/elio /usr/local/bin/elio

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data
ENTRYPOINT ["/entrypoint.sh"]
CMD ["elio", "/bench/data/src"]
