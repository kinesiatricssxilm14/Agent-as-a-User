# ============================================================
# P08 · phlx0/snip (PYTHON)
# ============================================================
FROM python:3.13-slim
LABEL bench.id="P08" bench.slug="snip" bench.lang="python"

RUN apt-get update -qq && apt-get install -y --no-install-recommends git sqlite3 2>/dev/null && rm -rf /var/lib/apt/lists/*

ARG SOURCE_SHA=52b2ec6b59ca77e2896b286a5d3d36b2460e12b2
RUN git clone https://github.com/phlx0/snip /src \
 && cd /src && git fetch --depth 1 origin ${SOURCE_SHA} && git checkout ${SOURCE_SHA}
RUN pip install --no-cache-dir /src

RUN mkdir -p /bench/data /bench/config
COPY seed/ /bench/
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

WORKDIR /bench/data
ENTRYPOINT ["/entrypoint.sh"]
CMD ["snip"]
