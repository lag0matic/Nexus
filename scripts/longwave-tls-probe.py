#!/usr/bin/env python3
"""WSL loopback TLS checks against a Windows longwave_tls_probe.exe.

Generates fresh throwaway certificates, never reads operator credentials, and
checks that a wrong/missing pin prevents HTTP authentication headers being sent.
"""

import argparse
import json
from pathlib import Path
import socketserver
import ssl
import subprocess
import tempfile
import threading


def certificate(directory, name):
    cert = directory / (name + ".pem")
    key = directory / (name + ".key")
    subprocess.run(
        ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
         "-subj", "/CN=longwave-test.invalid", "-keyout", str(key), "-out", str(cert)],
        check=True, capture_output=True,
    )
    der = subprocess.run(
        ["openssl", "x509", "-in", str(cert), "-outform", "DER"],
        check=True, capture_output=True,
    ).stdout
    return cert, key, der


def check(client, directory, context, pin, expected, label):
    requests = []

    class Handler(socketserver.StreamRequestHandler):
        def handle(self):
            self.request.settimeout(5)
            lines = []
            for _ in range(64):
                line = self.rfile.readline(8192)
                if not line or line == b"\r\n":
                    break
                lines.append(line)
            requests.append(lines)
            self.wfile.write(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")

    class Server(socketserver.TCPServer):
        def get_request(self):
            connection, address = super().get_request()
            connection.settimeout(5)
            try:
                return context.wrap_socket(connection, server_side=True), address
            except Exception:
                connection.close()
                raise

    with Server(("127.0.0.1", 0), Handler) as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            origin = "https://127.0.0.1:" + str(server.server_address[1])
            trust = "-"
            if pin is not None:
                path = directory / "trust.json"
                path.write_text(json.dumps({"version": 1, "servers": [
                    {"origin": origin, "certificate_der": list(pin)}
                ]}), encoding="utf-8")
                trust = subprocess.run(["wslpath", "-w", str(path)], check=True, capture_output=True, text=True).stdout.strip()
            result = subprocess.run([str(client), origin, trust, expected], capture_output=True, text=True, timeout=15)
            if result.returncode:
                raise AssertionError(label + ": " + result.stdout + result.stderr)
            if len(requests) != (1 if expected == "credentials" else 0):
                raise AssertionError(label + ": unexpected HTTP request count")
            if requests and not any(line.lower().startswith(b"x-api-key: local-tls-check-") for line in requests[0]):
                raise AssertionError(label + ": missing generated diagnostic header")
            print("PASS:", label, "HTTP requests:", len(requests), flush=True)
        finally:
            server.shutdown()
            thread.join(timeout=5)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--client", type=Path, required=True)
    args = parser.parse_args()
    client = args.client.resolve(strict=True)
    target = Path(__file__).resolve().parent.parent / "target"
    target.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="longwave-tls-", dir=target) as temp:
        directory = Path(temp).resolve()
        if not directory.is_relative_to(target.resolve()):
            raise RuntimeError("temporary TLS fixtures escaped the build directory")
        cert, key, approved = certificate(directory, "approved")
        _, _, wrong = certificate(directory, "other")
        for version in [ssl.TLSVersion.TLSv1_2, ssl.TLSVersion.TLSv1_3]:
            context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            context.minimum_version = context.maximum_version = version
            context.load_cert_chain(cert, key)
            check(client, directory, context, approved, "credentials", version.name + " exact pin / mismatched DNS name")
            check(client, directory, context, wrong, "certificate", version.name + " wrong pin blocks credentials")
            check(client, directory, context, None, "certificate", version.name + " missing approval blocks credentials")


if __name__ == "__main__":
    main()
