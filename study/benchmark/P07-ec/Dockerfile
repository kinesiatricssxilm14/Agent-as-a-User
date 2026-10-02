# ============================================================
# P07 · chojs23/ec (GO)
# ============================================================
FROM golang:1.26-alpine AS builder

RUN apk add --no-cache git ca-certificates

ARG SOURCE_SHA=d7176849d750ede014c6e7a50bab0f8bf36e4600
RUN git clone https://github.com/chojs23/ec /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
WORKDIR /src
RUN go build -ldflags="-s -w" -o /out/ec ./cmd/ec
FROM alpine:3.19
LABEL bench.id="P07" bench.slug="ec" bench.lang="go"

RUN apk add --no-cache ca-certificates git bash

COPY --from=builder /out/ec /usr/local/bin/ec

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data/repo
ENTRYPOINT ["/entrypoint.sh"]
CMD ["ec", "conflict.py"]
