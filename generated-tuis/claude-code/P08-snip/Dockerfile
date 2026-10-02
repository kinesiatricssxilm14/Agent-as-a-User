# toolh - Debian 12 slim, matching the documented runtime environment.
FROM debian:12-slim

ENV DEBIAN_FRONTEND=noninteractive \
    PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    TERM=xterm-256color \
    # Snippets live on a volume so they outlive the container.
    TOOLH_DB=/data/snippets.db

# git and sqlite3 are part of the documented base environment; xclip lets the
# clipboard work when an X display happens to be available (OSC 52 covers the
# headless case).
RUN apt-get update && apt-get install -y --no-install-recommends \
        python3 \
        python3-pip \
        git \
        sqlite3 \
        xclip \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY pyproject.toml README.md ./
COPY src ./src
RUN pip3 install --no-cache-dir --break-system-packages .

RUN mkdir -p /data
VOLUME ["/data"]
WORKDIR /root

# Default launch command is `toolh`, with no path arguments.
CMD ["toolh"]
