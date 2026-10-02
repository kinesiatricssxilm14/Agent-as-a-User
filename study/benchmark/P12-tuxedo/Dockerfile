# ============================================================
# P12 · webstonehq/tuxedo (RUST)
# ============================================================
FROM rust:1-bookworm AS builder

RUN apt-get update -qq && apt-get install -y \
    git pkg-config libssl-dev libsqlite3-dev \
    2>/dev/null \
 && rm -rf /var/lib/apt/lists/*

ARG SOURCE_SHA=06c7f8caf5afc95fb1a1079f23ac310d48fc7020
RUN git clone https://github.com/webstonehq/tuxedo /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
WORKDIR /src
RUN cargo build --release
FROM debian:12-slim
LABEL bench.id="P12" bench.slug="tuxedo" bench.lang="rust"

RUN apt-get update -qq \
 && apt-get install -y --no-install-recommends \
    ca-certificates libssl3
RUN rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/tuxedo /usr/local/bin/tuxedo

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENV TODO_FILE=/bench/data/todo.txt
WORKDIR /bench/data
ENTRYPOINT ["/entrypoint.sh"]
CMD ["tuxedo"]
