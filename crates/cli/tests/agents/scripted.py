"""An agent whose every turn is written in its prompt, for the policy and exit-code tests.

Each prompt is a JSON object:

- `{"stopReason": "refusal"}` ends the turn with that stop reason.
- `{"permission": ["allow_always", "reject_once"]}` first asks permission with
  one option of each kind listed (its id is the kind), then ends the turn with
  `end_turn` when an option was selected and `cancelled` when the request was
  answered `cancelled` or the turn was cancelled.

Standard library only.
"""
import json
import sys


def emit(frame):
    sys.stdout.write(json.dumps(frame) + "\n")
    sys.stdout.flush()


def answer(ident, stop_reason):
    emit({"jsonrpc": "2.0", "id": ident, "result": {"stopReason": stop_reason}})


def ask(kinds):
    """Asks permission and reads until the request is answered or the turn cancelled."""
    emit({"jsonrpc": "2.0", "id": "permission", "method": "session/request_permission", "params": {
        "sessionId": "s1",
        "toolCall": {"toolCallId": "call-1", "title": "Write a file"},
        "options": [{"optionId": kind, "name": kind, "kind": kind} for kind in kinds]}})
    for line in sys.stdin:
        frame = json.loads(line)
        if frame.get("id") == "permission" and "method" not in frame:
            outcome = frame.get("result", {}).get("outcome", {}).get("outcome")
            return "end_turn" if outcome == "selected" else "cancelled"
        if frame.get("method") == "session/cancel":
            return "cancelled"
    sys.exit(0)


for line in sys.stdin:
    frame = json.loads(line)
    method = frame.get("method")
    if method == "initialize":
        emit({"jsonrpc": "2.0", "id": frame["id"], "result": {"protocolVersion": 1}})
    elif method == "session/new":
        emit({"jsonrpc": "2.0", "id": frame["id"], "result": {"sessionId": "s1"}})
    elif method == "session/prompt":
        script = json.loads(frame["params"]["prompt"][0]["text"])
        if "permission" in script:
            answer(frame["id"], ask(script["permission"]))
        else:
            answer(frame["id"], script["stopReason"])
