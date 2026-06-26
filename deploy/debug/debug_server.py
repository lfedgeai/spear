import argparse
import json
import os
import threading
import time
from http.server import BaseHTTPRequestHandler, HTTPServer


class State:
    def __init__(self, session: str, outdir: str):
        self.session = session
        self.outdir = outdir
        self.last_activity = time.time()
        self.lock = threading.Lock()

    def log_path(self) -> str:
        return os.path.join(self.outdir, f"trae-debug-log-{self.session}.ndjson")

    def touch(self) -> None:
        with self.lock:
            self.last_activity = time.time()

    def write_env(self, host: str, port: int) -> None:
        os.makedirs(self.outdir, exist_ok=True)
        env_path = os.path.join(self.outdir, f"{self.session}.env")
        url = f"http://{host}:{port}/event"
        content = f"DEBUG_SERVER_URL={url}\nDEBUG_SESSION_ID={self.session}\n"
        with open(env_path, "w", encoding="utf-8") as f:
            f.write(content)


def _read_json(handler: BaseHTTPRequestHandler):
    length = int(handler.headers.get("content-length", "0") or "0")
    if length <= 0:
        return None
    raw = handler.rfile.read(length)
    return json.loads(raw.decode("utf-8"))


class Handler(BaseHTTPRequestHandler):
    state: State = None

    def _send_json(self, code: int, payload):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        self.state.touch()
        if self.path.startswith("/health"):
            self._send_json(200, {"ok": True, "session": self.state.session})
            return

        if self.path.startswith("/logs"):
            limit = 200
            if "?" in self.path:
                _, q = self.path.split("?", 1)
                for kv in q.split("&"):
                    if kv.startswith("limit="):
                        try:
                            limit = int(kv.split("=", 1)[1])
                        except Exception:
                            limit = 200
            try:
                with open(self.state.log_path(), "rb") as f:
                    lines = f.read().splitlines()[-limit:]
                self._send_json(200, {"lines": [l.decode("utf-8") for l in lines]})
            except FileNotFoundError:
                self._send_json(200, {"lines": []})
            return

        self._send_json(404, {"ok": False, "message": "not found"})

    def do_DELETE(self):
        self.state.touch()
        if self.path.startswith("/logs"):
            try:
                os.makedirs(self.state.outdir, exist_ok=True)
                with open(self.state.log_path(), "w", encoding="utf-8") as _:
                    pass
            except Exception as e:
                self._send_json(500, {"ok": False, "message": str(e)})
                return
            self._send_json(200, {"ok": True})
            return
        self._send_json(404, {"ok": False, "message": "not found"})

    def do_POST(self):
        self.state.touch()
        if not self.path.startswith("/event"):
            self._send_json(404, {"ok": False, "message": "not found"})
            return
        try:
            event = _read_json(self)
            if not isinstance(event, dict):
                raise ValueError("invalid json payload")
            if not event.get("sessionId"):
                event["sessionId"] = self.state.session
            if not event.get("ts"):
                event["ts"] = int(time.time() * 1000)
            os.makedirs(self.state.outdir, exist_ok=True)
            with open(self.state.log_path(), "a", encoding="utf-8") as f:
                f.write(json.dumps(event, ensure_ascii=False) + "\n")
            self._send_json(200, {"ok": True})
        except Exception as e:
            self._send_json(400, {"ok": False, "message": str(e)})

    def log_message(self, format, *args):
        return


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--host", default="0.0.0.0")
    p.add_argument("--port", type=int, default=7777)
    p.add_argument("--session", required=True)
    p.add_argument("--outdir", required=True)
    p.add_argument("--idle", type=int, default=1200)
    args = p.parse_args()

    state = State(args.session, args.outdir)
    Handler.state = state
    server = HTTPServer((args.host, args.port), Handler)
    state.write_env("debug-server", args.port)

    def watchdog():
        while True:
            time.sleep(2)
            if args.idle <= 0:
                continue
            with state.lock:
                idle = time.time() - state.last_activity
            if idle > args.idle:
                try:
                    server.shutdown()
                except Exception:
                    pass
                return

    t = threading.Thread(target=watchdog, daemon=True)
    t.start()
    server.serve_forever()


if __name__ == "__main__":
    main()
