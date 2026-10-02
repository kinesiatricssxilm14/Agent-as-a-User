# ============================================================
# P05 · brevity1swos/rgx (RUST)
# ============================================================
FROM rust:1-bookworm AS builder

RUN apt-get update -qq && apt-get install -y \
    git pkg-config libssl-dev libsqlite3-dev \
    2>/dev/null \
 && rm -rf /var/lib/apt/lists/*

ARG SOURCE_SHA=4a4193e7aae9acc12cb77a4fba9000e8a1a3b26a
RUN git clone https://github.com/brevity1swos/rgx /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
WORKDIR /src
RUN cargo build --release
FROM debian:12-slim
LABEL bench.id="P05" bench.slug="rgx" bench.lang="rust"

RUN apt-get update -qq \
 && apt-get install -y --no-install-recommends \
    ca-certificates libssl3
RUN rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/rgx /usr/local/bin/rgx

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data
ENTRYPOINT ["/entrypoint.sh"]
CMD ["rgx", "-f", "/bench/data/input.txt"]
