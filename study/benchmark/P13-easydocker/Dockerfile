# ============================================================
# P13 · joao-zanutto/easydocker (GO) — Mock Docker API mode
# ============================================================
FROM golang:1.26-alpine AS builder

RUN apk add --no-cache git ca-certificates

ARG SOURCE_SHA=d0b28f9ecc3729d456812e852903e36fe7978c04
RUN git clone https://github.com/joao-zanutto/easydocker /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
WORKDIR /src
RUN go build -ldflags="-s -w" -o /out/easydocker ./cmd/easydocker

FROM alpine:3.19
LABEL bench.id="P13" bench.slug="easydocker" bench.lang="go"

RUN apk add --no-cache ca-certificates python3 bash

COPY --from=builder /out/easydocker /usr/local/bin/easydocker
RUN chmod +x /usr/local/bin/easydocker

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh && chmod +x /bench/mock_docker_api.py

WORKDIR /bench/data
ENTRYPOINT ["/entrypoint.sh"]
CMD ["easydocker"]
