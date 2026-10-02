# ============================================================
# P01 · mexirica/aptui (GO)
# ============================================================
FROM golang:1.26-alpine AS builder

RUN apk add --no-cache git ca-certificates

ARG SOURCE_SHA=ed90731a7f91f132cfcead45917fc6b1d0ec688d
RUN git clone https://github.com/mexirica/aptui.git /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
WORKDIR /src
RUN go build -ldflags="-s -w" -o /out/aptui .
FROM debian:12-slim
LABEL bench.id="P01" bench.slug="aptui" bench.lang="go"

RUN apt-get update -qq && apt-get install -y --no-install-recommends ca-certificates  apt-utils sudo

COPY --from=builder /out/aptui /usr/local/bin/aptui

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data

ENTRYPOINT ["/entrypoint.sh"]

CMD ["sudo", "aptui"]
