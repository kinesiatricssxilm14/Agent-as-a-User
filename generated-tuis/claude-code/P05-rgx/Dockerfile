# Build and run `toole` in the environment it targets: Debian 12 slim, root.
#
#   docker build -t toole .
#   docker run --rm -it toole

FROM rust:1-slim-bookworm AS build
WORKDIR /src

# Copy the manifests first so dependency compilation is cached independently of
# source edits.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

COPY src ./src
COPY tests ./tests
COPY README.md ./
# Touch the real entry point so cargo rebuilds it over the placeholder.
RUN touch src/main.rs && cargo install --path . --locked --root /usr/local

FROM debian:12-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /usr/local/bin/toole /usr/local/bin/toole

# The default input file documented for this tool.
RUN mkdir -p /bench/data
COPY data/input.txt /bench/data/input.txt

ENV TERM=xterm-256color
WORKDIR /bench
CMD ["toole", "-f", "/bench/data/input.txt"]
