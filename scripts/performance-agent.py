#!/usr/bin/env python3
"""Manual native stress: prompt 'stream' or 'large'; standard library only."""
import json
import sys
import time


def emit(value):
    print(json.dumps(value, ensure_ascii=False, separators=(",", ":")), flush=True)


for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    result = {}
    if method == "initialize":
        result = {"protocolVersion": 1, "agentCapabilities": {}}
    elif method == "session/new":
        result = {"sessionId": "stress"}
    elif method == "session/prompt":
        large = "large" in str(request["params"]["prompt"])
        for i in range(8 if large else 15000):
            if large:
                emit({"jsonrpc": "2.0", "method": "stress/large", "params": {"ordinal": i, "text": "x" * (9 * 1024 * 1024) + "🦀END"}})
            else:
                emit({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "stress", "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": str(i) + " " + "x" * 256}}}})
                print(str(i) + ":" + "e" * 1024, file=sys.stderr, flush=True)
                if i % 100 == 0:
                    time.sleep(0.01)
        result = {"stopReason": "end_turn"}
    if "id" in request:
        emit({"jsonrpc": "2.0", "id": request["id"], "result": result})
