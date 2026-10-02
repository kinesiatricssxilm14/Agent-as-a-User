# ============================================================
# P02 · jsubroto/flow (RUST)
# ============================================================
FROM rust:1-bookworm AS builder

RUN apt-get update -qq && apt-get install -y \
    git pkg-config libssl-dev libsqlite3-dev \
    2>/dev/null \
 && rm -rf /var/lib/apt/lists/*

ARG SOURCE_SHA=8180ccf881efee92113b2e2816fa6d34e5d60947
RUN git clone https://github.com/jsubroto/flow /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
WORKDIR /src
RUN cargo build --release
FROM debian:12-slim
LABEL bench.id="P02" bench.slug="flow" bench.lang="rust"

RUN apt-get update -qq \
 && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 vim
RUN rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/flow /usr/local/bin/flow

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh /bench/init.sh 2>/dev/null || true

WORKDIR /bench/data
ENV FLOW_BOARD_PATH=/bench/data/board
ENTRYPOINT ["/entrypoint.sh"]
CMD ["flow"]
