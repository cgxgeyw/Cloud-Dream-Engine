from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.error import HTTPError, URLError
from urllib.parse import urljoin
from urllib.request import Request, urlopen


PORT = 39081
UPSTREAMS = {
    "/maven/": "https://mirrors.huaweicloud.com/repository/maven/",
    "/plugins/": "https://plugins.gradle.org/m2/",
}
HOP_BY_HOP = {
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
}


class ProxyHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        self._proxy()

    def do_HEAD(self):
        self._proxy(head_only=True)

    def log_message(self, fmt, *args):
        return

    def _proxy(self, head_only=False):
        upstream_base = None
        upstream_path = None
        for prefix, base in UPSTREAMS.items():
            if self.path.startswith(prefix):
                upstream_base = base
                upstream_path = self.path[len(prefix) :]
                break

        if upstream_base is None:
            self.send_error(404, "unknown route")
            return

        request = Request(
            urljoin(upstream_base, upstream_path),
            headers={
                "User-Agent": "curl/8.0",
                "Accept-Encoding": "identity",
            },
            method="GET" if head_only else self.command,
        )

        try:
            with urlopen(request, timeout=60) as response:
                self.send_response(response.status)
                for key, value in response.headers.items():
                    if key.lower() in HOP_BY_HOP:
                        continue
                    self.send_header(key, value)
                self.end_headers()

                if head_only:
                    return

                while True:
                    chunk = response.read(64 * 1024)
                    if not chunk:
                        break
                    self.wfile.write(chunk)
        except HTTPError as error:
            self.send_response(error.code)
            for key, value in error.headers.items():
                if key.lower() in HOP_BY_HOP:
                    continue
                self.send_header(key, value)
            self.end_headers()
            if not head_only:
                body = error.read()
                if body:
                    self.wfile.write(body)
        except (URLError, OSError):
            self.send_error(502, "upstream fetch failed")


def main():
    server = ThreadingHTTPServer(("127.0.0.1", PORT), ProxyHandler)
    server.serve_forever()


if __name__ == "__main__":
    main()
