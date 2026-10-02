#!/usr/bin/env python3
"""Mock Docker API server for easydocker benchmark (no real Docker needed)."""

import json
import os
import signal
import socket
import sys
import threading
from http.server import HTTPServer, BaseHTTPRequestHandler
from socketserver import UnixStreamServer

SOCK_PATH = "/var/run/docker.sock"

MOCK_CONTAINERS = [
    {
        "Id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
        "Names": ["/bench-nginx"],
        "Image": "nginx:alpine",
        "ImageID": "sha256:abc123def456abc123def456abc123def456abc123def456abc123def456abc1",
        "Command": "/docker-entrypoint.sh nginx -g 'daemon off;'",
        "Created": 1719000000,
        "State": "running",
        "Status": "Up 2 hours",
        "Ports": [{"PrivatePort": 80, "Type": "tcp"}],
        "Labels": {},
        "SizeRw": 0,
        "SizeRootFs": 0,
    },
    {
        "Id": "b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3",
        "Names": ["/bench-redis"],
        "Image": "redis:alpine",
        "ImageID": "sha256:def789abc123def789abc123def789abc123def789abc123def789abc123def7",
        "Command": "docker-entrypoint.sh redis-server",
        "Created": 1718990000,
        "State": "exited",
        "Status": "Exited (0) 30 minutes ago",
        "Ports": [],
        "Labels": {},
        "SizeRw": 0,
        "SizeRootFs": 0,
    },
    {
        "Id": "c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4",
        "Names": ["/bench-mysql"],
        "Image": "mysql:8.0",
        "ImageID": "sha256:789abc123def789abc123def789abc123def789abc123def789abc123def789a",
        "Command": "docker-entrypoint.sh mysqld",
        "Created": 1718980000,
        "State": "running",
        "Status": "Up 5 hours",
        "Ports": [{"PrivatePort": 3306, "Type": "tcp"}],
        "Labels": {},
        "SizeRw": 0,
        "SizeRootFs": 0,
    },
]

MOCK_CONTAINER_DETAIL = {
    "a1b2c3d4e5f6": {
        "Id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
        "Created": "2026-06-21T10:00:00Z",
        "Path": "/docker-entrypoint.sh",
        "Args": ["nginx", "-g", "daemon off;"],
        "State": {
            "Status": "running",
            "Running": True,
            "Paused": False,
            "Restarting": False,
            "OOMKilled": False,
            "Dead": False,
            "Pid": 1234,
            "ExitCode": 0,
            "Error": "",
            "StartedAt": "2026-06-21T10:00:01Z",
            "FinishedAt": "0001-01-01T00:00:00Z",
        },
        "Image": "sha256:abc123def456abc123def456abc123def456abc123def456abc123def456abc1",
        "Name": "/bench-nginx",
        "HostConfig": {"NetworkMode": "default"},
        "NetworkSettings": {"Networks": {}},
        "Mounts": [],
    },
    "b2c3d4e5f6a1": {
        "Id": "b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3",
        "Created": "2026-06-21T08:00:00Z",
        "Path": "docker-entrypoint.sh",
        "Args": ["redis-server"],
        "State": {
            "Status": "exited",
            "Running": False,
            "Paused": False,
            "Restarting": False,
            "OOMKilled": False,
            "Dead": False,
            "Pid": 0,
            "ExitCode": 0,
            "Error": "",
            "StartedAt": "2026-06-21T08:00:01Z",
            "FinishedAt": "2026-06-21T11:30:00Z",
        },
        "Image": "sha256:def789abc123def789abc123def789abc123def789abc123def789abc123def7",
        "Name": "/bench-redis",
        "HostConfig": {"NetworkMode": "default"},
        "NetworkSettings": {"Networks": {}},
        "Mounts": [],
    },
    "c3d4e5f6a1b2": {
        "Id": "c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4",
        "Created": "2026-06-21T06:00:00Z",
        "Path": "docker-entrypoint.sh",
        "Args": ["mysqld"],
        "State": {
            "Status": "running",
            "Running": True,
            "Paused": False,
            "Restarting": False,
            "OOMKilled": False,
            "Dead": False,
            "Pid": 5678,
            "ExitCode": 0,
            "Error": "",
            "StartedAt": "2026-06-21T06:00:01Z",
            "FinishedAt": "0001-01-01T00:00:00Z",
        },
        "Image": "sha256:789abc123def789abc123def789abc123def789abc123def789abc123def789a",
        "Name": "/bench-mysql",
        "HostConfig": {"NetworkMode": "default"},
        "NetworkSettings": {"Networks": {}},
        "Mounts": [],
    },
}

MOCK_CONTAINER_LOGS = {
    "a1b2c3d4e5f6": (
        "/docker-entrypoint.sh: /docker-entrypoint.d/ is not empty, will attempt to perform configuration\n"
        "/docker-entrypoint.sh: Looking for shell scripts in /docker-entrypoint.d/\n"
        "/docker-entrypoint.sh: Launching /docker-entrypoint.d/10-listen-on-ipv6-by-default.sh\n"
        "10-listen-on-ipv6-by-default.sh: info: IPv6 listen already enabled\n"
        "/docker-entrypoint.sh: Configuration complete; ready for start up\n"
        "2026/06/21 10:00:02 [notice] 1#1: start worker process 1\n"
        "2026/06/21 10:00:02 [notice] 1#1: start worker process 2\n"
        "2026/06/21 10:00:02 [notice] 1#1: start worker process 3\n"
        "2026/06/21 10:00:02 [notice] 1#1: start worker process 4\n"
    ),
    "b2c3d4e5f6a1": (
        "1:C 21 Jun 2026 08:00:02.000 * oO0OoO0OoO0Oo Redis is starting oO0OoO0OoO0Oo\n"
        "1:C 21 Jun 2026 08:00:02.000 * Redis version=7.2.5, bits=64, commit=00000000, modified=0, pid=1\n"
        "1:C 21 Jun 2026 08:00:02.000 * Configuration loaded\n"
        "1:M 21 Jun 2026 08:00:02.000 * Ready to accept connections tcp\n"
    ),
    "c3d4e5f6a1b2": (
        "2026-06-21T06:00:02.000000Z 0 [System] [MY-010931] [Server] /usr/sbin/mysqld: ready for connections.\n"
        "2026-06-21T06:00:02.000000Z 0 [System] [MY-011323] [Server] X Plugin ready for connections.\n"
    ),
}

MOCK_IMAGES = [
    {
        "Id": "sha256:abc123def456abc123def456abc123def456abc123def456abc123def456abc1",
        "RepoTags": ["nginx:alpine"],
        "RepoDigests": [],
        "Created": 1718500000,
        "Size": 45000000,
        "SharedSize": 0,
        "VirtualSize": 45000000,
        "Labels": {},
    },
    {
        "Id": "sha256:def789abc123def789abc123def789abc123def789abc123def789abc123def7",
        "RepoTags": ["redis:alpine"],
        "RepoDigests": [],
        "Created": 1718400000,
        "Size": 35000000,
        "SharedSize": 0,
        "VirtualSize": 35000000,
        "Labels": {},
    },
    {
        "Id": "sha256:789abc123def789abc123def789abc123def789abc123def789abc123def789a",
        "RepoTags": ["mysql:8.0"],
        "RepoDigests": [],
        "Created": 1718300000,
        "Size": 150000000,
        "SharedSize": 0,
        "VirtualSize": 150000000,
        "Labels": {},
    },
    {
        "Id": "sha256:aaa111bbb222ccc333ddd444eee555fff666ggg777hhh888iii999jjj000kkk",
        "RepoTags": ["alpine:3.19"],
        "RepoDigests": [],
        "Created": 1718200000,
        "Size": 8000000,
        "SharedSize": 0,
        "VirtualSize": 8000000,
        "Labels": {},
    },
]

MOCK_NETWORKS = [
    {
        "Name": "bridge",
        "Id": "net001bridge001bridge001bridge001bridge001bridge001bridge001bridge001",
        "Created": "2026-06-01T00:00:00Z",
        "Scope": "local",
        "Driver": "bridge",
        "EnableIPv6": False,
        "IPAM": {"Driver": "default", "Config": [{"Subnet": "172.17.0.0/16"}]},
        "Internal": False,
        "Attachable": False,
        "Ingress": False,
        "Containers": {},
    },
    {
        "Name": "host",
        "Id": "net002host0002net002host0002net002host0002net002host0002net002ho",
        "Created": "2026-06-01T00:00:00Z",
        "Scope": "local",
        "Driver": "host",
        "EnableIPv6": False,
        "IPAM": {"Driver": "default", "Config": []},
        "Internal": False,
        "Attachable": False,
        "Ingress": False,
        "Containers": {},
    },
    {
        "Name": "none",
        "Id": "net003none0003net003none0003net003none0003net003none0003net003no",
        "Created": "2026-06-01T00:00:00Z",
        "Scope": "local",
        "Driver": "null",
        "EnableIPv6": False,
        "IPAM": {"Driver": "default", "Config": []},
        "Internal": False,
        "Attachable": False,
        "Ingress": False,
        "Containers": {},
    },
]

MOCK_VOLUMES = [
    {
        "Name": "bench-data",
        "Driver": "local",
        "Mountpoint": "/var/lib/docker/volumes/bench-data/_data",
        "CreatedAt": "2026-06-20T10:00:00Z",
        "Labels": {},
        "Scope": "local",
        "Options": {},
    },
    {
        "Name": "bench-logs",
        "Driver": "local",
        "Mountpoint": "/var/lib/docker/volumes/bench-logs/_data",
        "CreatedAt": "2026-06-20T11:00:00Z",
        "Labels": {},
        "Scope": "local",
        "Options": {},
    },
]


MOCK_IMAGE_DETAIL = {
    "abc123def456": {
        "Id": "sha256:abc123def456abc123def456abc123def456abc123def456abc123def456abc1",
        "RepoTags": ["nginx:alpine"],
        "RepoDigests": [],
        "Created": "2026-06-15T00:00:00Z",
        "Container": "",
        "ContainerConfig": {},
        "DockerVersion": "24.0.5",
        "Architecture": "amd64",
        "Os": "linux",
        "Size": 45000000,
        "RootFS": {
            "Type": "layers",
            "Layers": [
                "sha256:layer1abc123",
                "sha256:layer2def456",
                "sha256:layer3ghi789",
            ],
        },
    },
}


class MockDockerHandler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass

    @staticmethod
    def _strip_version(path):
        """Strip /v1.XX prefix from Docker API paths."""
        if path.startswith("/v"):
            idx = path.find("/", 1)
            if idx > 0:
                return path[idx:]
        return path

    def _respond(self, code, data):
        body = json.dumps(data).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
        self.wfile.flush()

    def _respond_raw(self, code, data):
        body = data.encode() if isinstance(data, str) else data
        self.send_response(code)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
        self.wfile.flush()

    def do_GET(self):
        raw_path = self.path.split("?")[0]
        path = self._strip_version(raw_path)

        if path == "/_ping":
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            self.send_header("Content-Length", "2")
            self.end_headers()
            self.wfile.write(b"OK")
            self.wfile.flush()
            return

        if path == "/containers/json":
            self._respond(200, MOCK_CONTAINERS)
            return

        if path == "/images/json":
            self._respond(200, MOCK_IMAGES)
            return

        if path == "/networks":
            self._respond(200, MOCK_NETWORKS)
            return

        if path == "/volumes":
            self._respond(200, {"Volumes": MOCK_VOLUMES, "Warnings": []})
            return

        if path.startswith("/containers/"):
            parts = path.strip("/").split("/")
            if len(parts) >= 3:
                cid = parts[1]
                action = parts[2]
                if action == "json":
                    detail = MOCK_CONTAINER_DETAIL.get(cid[:12])
                    if detail:
                        self._respond(200, detail)
                        return
                    self._respond(404, {"message": "No such container"})
                    return
                if action == "logs":
                    log_data = MOCK_CONTAINER_LOGS.get(cid[:12], "")
                    self._respond_raw(200, log_data)
                    return

        if path.startswith("/images/"):
            parts = path.strip("/").split("/")
            if len(parts) >= 3 and parts[2] == "json":
                img_ref = parts[1]
                detail = MOCK_IMAGE_DETAIL.get(img_ref[:12])
                if detail:
                    self._respond(200, detail)
                    return
                for img in MOCK_IMAGES:
                    if img_ref in (img["Id"], img.get("RepoTags", [""])[0]):
                        self._respond(200, img)
                        return
                self._respond(404, {"message": "No such image"})
                return

        if path == "/version":
            self._respond(200, {
                "Version": "24.0.7",
                "ApiVersion": "1.43",
                "MinAPIVersion": "1.12",
            })
            return

        if path == "/info":
            self._respond(200, {
                "Containers": len(MOCK_CONTAINERS),
                "ContainersRunning": sum(1 for c in MOCK_CONTAINERS if c["State"] == "running"),
                "ContainersStopped": sum(1 for c in MOCK_CONTAINERS if c["State"] == "exited"),
                "Images": len(MOCK_IMAGES),
                "ServerVersion": "24.0.7",
            })
            return

        self._respond(404, {"message": "not found"})

    def do_POST(self):
        raw_path = self.path.split("?")[0]
        path = self._strip_version(raw_path)
        content_len = int(self.headers.get("Content-Length", 0))
        if content_len:
            self.rfile.read(content_len)

        if path.startswith("/containers/"):
            parts = path.strip("/").split("/")
            if len(parts) >= 3:
                cid = parts[1]
                action = parts[2]
                if action == "stop":
                    for c in MOCK_CONTAINERS:
                        if c["Id"].startswith(cid[:12]):
                            c["State"] = "exited"
                            c["Status"] = "Exited (0) Less than a second ago"
                            break
                    self.send_response(204)
                    self.end_headers()
                    return
                if action == "start":
                    for c in MOCK_CONTAINERS:
                        if c["Id"].startswith(cid[:12]):
                            c["State"] = "running"
                            c["Status"] = "Up Less than a second ago"
                            break
                    self.send_response(204)
                    self.end_headers()
                    return
                if action == "restart":
                    for c in MOCK_CONTAINERS:
                        if c["Id"].startswith(cid[:12]):
                            c["State"] = "running"
                            c["Status"] = "Up Less than a second ago"
                            break
                    self.send_response(204)
                    self.end_headers()
                    return
                if action in ("pause", "unpause"):
                    self.send_response(204)
                    self.end_headers()
                    return
                if action == "kill":
                    for c in MOCK_CONTAINERS:
                        if c["Id"].startswith(cid[:12]):
                            c["State"] = "exited"
                            c["Status"] = "Exited (137) Less than a second ago"
                            break
                    self.send_response(204)
                    self.end_headers()
                    return

        self._respond(404, {"message": "not found"})

    def do_DELETE(self):
        raw_path = self.path.split("?")[0]
        path = self._strip_version(raw_path)
        if path.startswith("/containers/"):
            cid = path.strip("/").split("/")[1]
            for i, c in enumerate(MOCK_CONTAINERS):
                if c["Id"].startswith(cid[:12]):
                    MOCK_CONTAINERS.pop(i)
                    self.send_response(204)
                    self.end_headers()
                    return
            self._respond(404, {"message": "No such container"})
            return
        self._respond(404, {"message": "not found"})


class UnixSocketHTTPServer(UnixStreamServer):
    address_family = socket.AF_UNIX

    def server_bind(self):
        self.socket.bind(self.server_address)


def main():
    if os.path.exists(SOCK_PATH):
        os.remove(SOCK_PATH)

    server = UnixSocketHTTPServer(SOCK_PATH, MockDockerHandler)
    os.chmod(SOCK_PATH, 0o666)

    def shutdown_handler(signum, frame):
        server.shutdown()
        sys.exit(0)

    signal.signal(signal.SIGTERM, shutdown_handler)
    signal.signal(signal.SIGINT, shutdown_handler)

    server.serve_forever()


if __name__ == "__main__":
    main()
