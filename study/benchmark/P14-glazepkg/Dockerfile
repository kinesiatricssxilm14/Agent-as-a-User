FROM golang:1.26-alpine AS builder

RUN apk add --no-cache git ca-certificates

ARG SOURCE_SHA=26164423f4cf8bbbd4afdcf38d04484bf069fd5a
RUN git clone https://github.com/neur0map/glazepkg /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
WORKDIR /src
RUN go build -ldflags="-s -w" -o /out/gpk ./cmd/gpk
FROM debian:12-slim
LABEL bench.id="P14" bench.slug="glazepkg" bench.lang="go"

RUN apt-get update -qq && apt-get install -y --no-install-recommends ca-certificates python3-pip python3 tree && rm -rf /var/lib/apt/lists/*

COPY --from=builder /out/gpk /usr/local/bin/gpk
RUN ln -s /usr/local/bin/gpk /usr/local/bin/glazepkg

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data

# Work around Debian 12 PEP 668 restrictions; point USERBASE to global site
ENV PIP_BREAK_SYSTEM_PACKAGES=1
ENV PYTHONUSERBASE="/usr/local"

ENTRYPOINT ["/entrypoint.sh"]
CMD ["gpk"]
