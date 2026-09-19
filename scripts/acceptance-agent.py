#!/usr/bin/env python3
"""Portable ACP stdio fixture; Python 3.9+, standard library only.

No MCP processes, network access or persistent Session storage. Stdout is UTF-8
JSONL only. See docs/native-acceptance.md for launch fields and scenario recipes.
"""
import argparse
import base64
import binascii
import hashlib
import heapq
import json
import math
import queue
import sys
import threading
import time

METHODS = {"initialize", "session/new", "session/list", "session/load",
           "session/resume", "session/close", "session/delete", "session/prompt"}
MAX_FRAME = 12 * 1024 * 1024


def selection(allowed):
    def parse(value):
        if value == "all":
            return set(allowed)
        if value == "none":
            return set()
        chosen = set(value.split(","))
        if not chosen or not chosen <= set(allowed):
            raise argparse.ArgumentTypeError("use all, none, or comma-separated " + ",".join(allowed))
        return chosen
    return parse


def delay(value):
    try:
        method, seconds = value.split("=", 1)
        seconds = float(seconds)
        if method not in METHODS or not math.isfinite(seconds) or not 0 <= seconds <= 30:
            raise ValueError()
        return method, seconds
    except ValueError:
        raise argparse.ArgumentTypeError("delay must be METHOD=SECONDS (0..30, a supported request)") from None


def options():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", action="version", version="acceptance-agent 1")
    parser.add_argument("--prompt-capabilities", type=selection(("image", "audio", "embeddedContext")), default="all")
    parser.add_argument("--mcp-capabilities", type=selection(("http", "sse")), default="all")
    parser.add_argument("--restore", choices=("both", "load", "resume", "none"), default="both")
    parser.add_argument("--delay", action="append", type=delay, default=[], metavar="METHOD=SECONDS")
    parser.add_argument("--refuse", action="append", choices=sorted(METHODS), default=[])
    parser.add_argument("--error-code", type=int, default=-32602)
    parser.add_argument("--hold-prompts", action="store_true", help="hold Turns until session/cancel; setup/list still respond")
    args = parser.parse_args()
    if not -(2 ** 31) <= args.error_code < 2 ** 31:
        parser.error("--error-code must fit a signed 32-bit JSON-RPC code")
    args.delay = dict(args.delay)
    return args


def read_frames(incoming):
    """A reader thread keeps cancellation responsive on Windows pipes too."""
    while True:
        line = sys.stdin.buffer.readline(MAX_FRAME + 1)
        if not line:
            incoming.put(None)
            return
        if len(line) > MAX_FRAME:
            incoming.put(ValueError("input Frame exceeds 12 MiB"))
            incoming.put(None)
            return
        try:
            incoming.put(json.loads(line.decode("utf-8")))
        except (ValueError, UnicodeError):
            incoming.put(ValueError("invalid UTF-8 JSON Frame"))


class InvalidParams(Exception):
    pass


def require(condition, message):
    if not condition:
        raise InvalidParams(message)


def block_receipt(block, advertised):
    require(isinstance(block, dict), "content block must be an object")
    kind = block.get("type")
    result = {"type": kind}
    if kind == "text":
        text = block.get("text")
        require(isinstance(text, str), "text must be a string")
        data = text.encode("utf-8")
    elif kind in ("image", "audio"):
        require(kind in advertised, kind + " was not advertised")
        result["mimeType"] = block.get("mimeType")
        require(isinstance(result["mimeType"], str), "mimeType must be a string")
        if "uri" in block:
            require(isinstance(block["uri"], str), "media URI must be a string")
            result["uri"] = block["uri"]
        data = decode_bytes(block.get("data"))
    elif kind == "resource":
        require("embeddedContext" in advertised, "embeddedContext was not advertised")
        resource = block.get("resource")
        require(isinstance(resource, dict), "resource must be an object")
        result["uri"] = resource.get("uri")
        require(isinstance(result["uri"], str), "resource URI must be a string")
        result["mimeType"] = resource.get("mimeType")
        if "text" in resource:
            require(isinstance(resource["text"], str), "resource text must be a string")
            data = resource["text"].encode("utf-8")
        else:
            data = decode_bytes(resource.get("blob"))
    else:
        raise InvalidParams("fixture accepts text, image, audio and embedded resource blocks")
    result.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
    return result


def decode_bytes(value):
    require(isinstance(value, str), "base64 payload must be a string")
    try:
        return base64.b64decode(value, validate=True)
    except (ValueError, binascii.Error):
        raise InvalidParams("invalid base64 payload") from None


class Agent:
    def __init__(self, args):
        self.args = args
        self.initialized = False
        self.sessions = {}
        self.next_session = 1
        self.pending = {}  # Session id -> (request id, receipt, completion token)
        self.scheduled = []
        self.sequence = 0

    def emit(self, frame):
        sys.stdout.buffer.write(json.dumps(frame, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + b"\n")
        sys.stdout.buffer.flush()

    def result(self, ident, result):
        self.emit({"jsonrpc": "2.0", "id": ident, "result": result})

    def error(self, ident, code, message):
        self.emit({"jsonrpc": "2.0", "id": ident, "error": {"code": code, "message": message}})

    def schedule(self, method, callback):
        self.sequence += 1
        heapq.heappush(self.scheduled, (time.monotonic() + self.args.delay.get(method, 0), self.sequence, callback))

    def complete(self, session, token, reason):
        pending = self.pending.get(session)
        if pending is None or pending[2] is not token:
            return
        ident, receipt, _ = self.pending.pop(session)
        if reason == "refused":
            self.error(ident, self.args.error_code, "fixture refused session/prompt")
            return
        if reason == "end_turn":
            frame = {"jsonrpc": "2.0", "method": "session/update", "params": {
                "sessionId": session, "update": {"sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": json.dumps(receipt, ensure_ascii=False)}}}}
            self.sessions[session]["history"].append(frame)
            self.emit(frame)
        self.result(ident, {"stopReason": reason})

    def mcp(self, params):
        definitions = params.get("mcpServers", [])
        require(isinstance(definitions, list), "mcpServers must be an array")
        summary = []
        for definition in definitions:
            require(isinstance(definition, dict), "MCP definition must be an object")
            kind = definition.get("type", "stdio")
            require(kind == "stdio" or kind in self.args.mcp_capabilities, "MCP transport was not advertised")
            name = definition.get("name")
            require(isinstance(name, str), "MCP name must be a string")
            if kind == "stdio":
                require(isinstance(definition.get("command"), str), "MCP command must be a string")
                args, env = definition.get("args", []), definition.get("env", [])
                require(isinstance(args, list) and all(isinstance(a, str) for a in args), "MCP args must be strings")
                pairs(env)
                summary.append({"name": name, "transport": kind, "arguments": len(args), "environment": len(env)})
            else:
                require(isinstance(definition.get("url"), str), "MCP URL must be a string")
                headers = definition.get("headers", [])
                pairs(headers)
                summary.append({"name": name, "transport": kind, "headers": len(headers)})
        return summary

    def handle(self, frame):
        if not isinstance(frame, dict) or frame.get("jsonrpc") != "2.0" or not isinstance(frame.get("method"), str):
            self.error(None, -32600, "expected a JSON-RPC request object")
            return
        method, params = frame["method"], frame.get("params", {})
        if "id" not in frame:
            if method == "session/cancel" and isinstance(params, dict):
                session = params.get("sessionId")
                if isinstance(session, str) and session in self.pending:
                    self.complete(session, self.pending[session][2], "cancelled")
            return
        ident = frame["id"]
        if method not in METHODS:
            self.error(ident, -32601, "method not implemented by acceptance fixture")
            return
        if not isinstance(params, dict):
            self.error(ident, -32602, "params must be an object")
            return
        if method in self.args.refuse and method != "session/prompt":
            self.schedule(method, lambda: self.error(ident, self.args.error_code, "fixture refused " + method))
            return
        try:
            if method == "session/prompt":
                require(self.initialized, "initialize first")
                session = self.session(params)
                require(session not in self.pending, "a Turn is already pending in this Session")
                blocks = params.get("prompt")
                require(isinstance(blocks, list), "prompt must be an array")
                receipt = {"fixture": "acceptance-agent", "sessionId": session,
                           "blocks": [block_receipt(block, self.args.prompt_capabilities) for block in blocks],
                           "mcpServers": self.sessions[session]["mcp"]}
                token = object()
                self.pending[session] = (ident, receipt, token)
                if method in self.args.refuse:
                    self.schedule(method, lambda: self.complete(session, token, "refused"))
                elif not self.args.hold_prompts:
                    self.schedule(method, lambda: self.complete(session, token, "end_turn"))
            else:
                self.schedule(method, lambda: self.execute(ident, method, params))
        except (InvalidParams, UnicodeError) as error:
            self.error(ident, -32602, str(error))

    def session(self, params):
        session = params.get("sessionId")
        require(isinstance(session, str) and session in self.sessions, "unknown Session")
        return session

    def execute(self, ident, method, params):
        try:
            if method == "initialize":
                require(params.get("protocolVersion") == 1, "fixture requires ACP v1")
                self.initialized = True
                caps = {"loadSession": self.args.restore in ("both", "load"),
                        "promptCapabilities": {k: k in self.args.prompt_capabilities for k in ("image", "audio", "embeddedContext")},
                        "mcpCapabilities": {k: k in self.args.mcp_capabilities for k in ("http", "sse")},
                        "sessionCapabilities": {"list": {}, "close": {}, "delete": {}}}
                if self.args.restore in ("both", "resume"):
                    caps["sessionCapabilities"]["resume"] = {}
                self.result(ident, {"protocolVersion": 1, "agentInfo": {"name": "acceptance-agent", "version": "1"}, "agentCapabilities": caps})
                return
            require(self.initialized, "initialize first")
            if method in ("session/new", "session/load", "session/resume"):
                cwd = params.get("cwd")
                require(isinstance(cwd, str) and bool(cwd), "cwd must be a nonempty string")
                mcp = self.mcp(params)
                if method == "session/new":
                    session = "acceptance-" + str(self.next_session)
                    self.next_session += 1
                    self.sessions[session] = {"cwd": cwd, "mcp": mcp, "history": []}
                    self.result(ident, {"sessionId": session})
                else:
                    kind = method.split("/")[1]
                    require(self.args.restore in ("both", kind), kind + " was not advertised")
                    session = self.session(params)
                    require(cwd == self.sessions[session]["cwd"], "cwd differs from the listed Session")
                    self.sessions[session]["mcp"] = mcp
                    if kind == "load":
                        for frame in self.sessions[session]["history"]:
                            self.emit(frame)
                    self.result(ident, {})
            elif method == "session/list":
                self.result(ident, {"sessions": [{"sessionId": sid, "cwd": state["cwd"], "title": "Acceptance " + sid} for sid, state in self.sessions.items()]})
            elif method in ("session/close", "session/delete"):
                session = self.session(params)
                if session in self.pending:
                    self.complete(session, self.pending[session][2], "cancelled")
                if method == "session/delete":
                    del self.sessions[session]
                self.result(ident, {})
        except InvalidParams as error:
            self.error(ident, -32602, str(error))

    def run(self):
        incoming = queue.Queue(maxsize=64)
        threading.Thread(target=read_frames, args=(incoming,), daemon=True).start()
        while True:
            now = time.monotonic()
            while self.scheduled and self.scheduled[0][0] <= now:
                _, _, callback = heapq.heappop(self.scheduled)
                callback()
            timeout = max(0, self.scheduled[0][0] - time.monotonic()) if self.scheduled else None
            try:
                frame = incoming.get(timeout=timeout)
            except queue.Empty:
                continue
            if frame is None:
                return
            if isinstance(frame, ValueError):
                self.error(None, -32700, str(frame))
            else:
                self.handle(frame)


def pairs(values):
    require(isinstance(values, list) and all(isinstance(v, dict) and isinstance(v.get("name"), str) and isinstance(v.get("value"), str) for v in values), "environment/headers must be name/value arrays")


if __name__ == "__main__":
    try:
        Agent(options()).run()
    except BrokenPipeError:
        # The inspector may close its read end while stopping the subprocess.
        sys.exit(0)
