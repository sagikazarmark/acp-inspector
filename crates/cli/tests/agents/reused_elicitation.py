"""An agent that breaks a MUST the inspector checks, so the CLI exits 2.

It answers the handshake, and on the first prompt opens two URL elicitations
under one `elicitationId` in a single write — the second while the first is
still outstanding — and then ends the turn. Standard library only.
"""
import json
import sys


def emit(*frames):
    sys.stdout.write("".join(json.dumps(frame) + "\n" for frame in frames))
    sys.stdout.flush()


def url(ident):
    return {"jsonrpc": "2.0", "id": ident, "method": "elicitation/create", "params": {
        "sessionId": "s1", "mode": "url", "message": "Sign in",
        "elicitationId": "e1", "url": "https://example.com/e1"}}


for line in sys.stdin:
    frame = json.loads(line)
    method = frame.get("method")
    if method == "initialize":
        emit({"jsonrpc": "2.0", "id": frame["id"], "result": {"protocolVersion": 1}})
    elif method == "session/new":
        emit({"jsonrpc": "2.0", "id": frame["id"], "result": {"sessionId": "s1"}})
    elif method == "session/prompt":
        emit(url("first"), url("second"),
             {"jsonrpc": "2.0", "id": frame["id"], "result": {"stopReason": "end_turn"}})
