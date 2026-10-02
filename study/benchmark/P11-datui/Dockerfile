# ============================================================
# P11 · derekwisong/datui (RUST)
# ============================================================
FROM rust:1-bookworm AS builder

RUN apt-get update -qq && apt-get install -y \
    git pkg-config libssl-dev libsqlite3-dev \
    2>/dev/null \
 && rm -rf /var/lib/apt/lists/*

ARG SOURCE_SHA=7f13e2f089774798cdc1cb545940a386e36b7481
RUN git clone https://github.com/derekwisong/datui /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
WORKDIR /src
# ethnum 1.5.1 (lockfile) fails on rustc >= 1.97; bump transitive dep only.
RUN cargo update -p ethnum --precise 1.5.3
# datui release profile uses codegen-units=1 (high RAM); relax for container builds.
ENV CARGO_PROFILE_RELEASE_CODEGEN_UNITS=4
RUN cargo build --release -j 2
FROM debian:12-slim
LABEL bench.id="P11" bench.slug="datui" bench.lang="rust"

RUN apt-get update -qq \
 && apt-get install -y --no-install-recommends libssl3 libfontconfig1 \
    ca-certificates libssl3
RUN rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/datui /usr/local/bin/datui

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
COPY seed/employees.csv /bench/data/employees.csv
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data
ENTRYPOINT ["/entrypoint.sh"]
CMD ["datui", "/bench/data/employees.csv"]
