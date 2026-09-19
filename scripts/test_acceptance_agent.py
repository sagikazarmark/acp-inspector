"""Portable subprocess checks: python -m unittest discover -s scripts -p 'test_*.py'."""
import base64
import hashlib
import json
from pathlib import Path
import queue
import subprocess
import sys
import threading
import unittest

AGENT = Path(__file__).with_name("acceptance-agent.py")


class Agent:
    def __init__(self, *args):
        self.process = subprocess.Popen(
            [sys.executable, "-u", str(AGENT), *args],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        self.frames = queue.Queue()
        self.errors = []
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.diagnostics = threading.Thread(target=self.drain, daemon=True)
        self.reader.start()
        self.diagnostics.start()

    def read(self):
        for line in self.process.stdout:
            try:
                self.frames.put(json.loads(line))
            except ValueError as error:
                self.frames.put(error)
        self.frames.put(EOFError("Agent exited"))

    def drain(self):
        for line in self.process.stderr:
            self.errors.append(line.decode("utf-8"))

    def send(self, method, params=None, ident=1):
        frame = {"jsonrpc": "2.0", "method": method, "params": params or {}}
        if ident is not None:
            frame["id"] = ident
        self.process.stdin.write(json.dumps(frame, ensure_ascii=False).encode("utf-8") + b"\n")
        self.process.stdin.flush()

    def next(self, timeout=3):
        frame = self.frames.get(timeout=timeout)
        if isinstance(frame, Exception):
            raise frame
        return frame

    def response(self, ident):
        updates = []
        while True:
            frame = self.next()
            if frame.get("id") == ident:
                return frame, updates
            updates.append(frame)

    def start(self):
        self.send("initialize", {"protocolVersion": 1})
        initialized, _ = self.response(1)
        self.send("session/new", {"cwd": str(AGENT.parent.resolve()), "mcpServers": []}, 2)
        opened, _ = self.response(2)
        return initialized, opened["result"]["sessionId"]

    def close(self):
        if self.process.poll() is None:
            self.process.stdin.close()
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=2)
        self.reader.join(timeout=2)
        self.diagnostics.join(timeout=2)
        for pipe in (self.process.stdin, self.process.stdout, self.process.stderr):
            pipe.close()


class AcceptanceAgentTests(unittest.TestCase):
    def agent(self, *args):
        agent = Agent(*args)
        self.addCleanup(agent.close)
        return agent

    def test_advertisement_combinations(self):
        for prompt in ("none", "image", "audio", "embeddedContext", "all"):
            with self.subTest(prompt=prompt):
                agent = self.agent("--prompt-capabilities", prompt, "--mcp-capabilities", "sse")
                initialized, _ = agent.start()
                caps = initialized["result"]["agentCapabilities"]
                self.assertEqual(caps["promptCapabilities"], {
                    key: prompt in (key, "all") for key in ("image", "audio", "embeddedContext")
                })
                self.assertEqual(caps["mcpCapabilities"], {"http": False, "sse": True})
                agent.close()

    def test_binary_and_utf8_receipts_preserve_order_and_hashes(self):
        agent = self.agent()
        _, session = agent.start()
        binary = b"%PDF-1.7\n\xff\x00\x80"
        text = '\ufeff// café\r\n<script>"\\'
        encoded = base64.b64encode(binary).decode("ascii")
        blocks = [
            {"type": "text", "text": "inspect"},
            {"type": "image", "mimeType": "image/png", "data": encoded, "uri": "file:///image.png"},
            {"type": "audio", "mimeType": "audio/wav", "data": encoded},
            {"type": "resource", "resource": {"uri": "file:///context%20%23.rs", "text": text, "mimeType": "text/plain"}},
            {"type": "resource", "resource": {"uri": "file:///report.pdf", "blob": encoded, "mimeType": "application/pdf"}},
        ]
        agent.send("session/prompt", {"sessionId": session, "prompt": blocks}, 3)
        answer, updates = agent.response(3)
        self.assertEqual(answer["result"]["stopReason"], "end_turn")
        receipt = json.loads(updates[-1]["params"]["update"]["content"]["text"])
        self.assertEqual([b["type"] for b in receipt["blocks"]], [b["type"] for b in blocks])
        for item, data in zip(receipt["blocks"], [b"inspect", binary, binary, text.encode("utf-8"), binary]):
            self.assertEqual(item["bytes"], len(data))
            self.assertEqual(item["sha256"], hashlib.sha256(data).hexdigest())
        self.assertEqual(receipt["blocks"][3]["uri"], "file:///context%20%23.rs")
        self.assertEqual(receipt["blocks"][1]["uri"], "file:///image.png")

    def test_load_replays_before_answer_resume_does_not(self):
        agent = self.agent()
        _, session = agent.start()
        agent.send("session/prompt", {"sessionId": session, "prompt": [{"type": "text", "text": "hello"}]}, 3)
        _, original = agent.response(3)
        agent.send("session/list", {}, 4)
        listing, _ = agent.response(4)
        self.assertEqual(listing["result"]["sessions"][0]["sessionId"], session)
        agent.send("session/load", {"sessionId": session, "cwd": str(AGENT.parent.resolve()), "mcpServers": []}, 5)
        answer, replay = agent.response(5)
        self.assertEqual(replay, original)
        self.assertEqual(answer["result"], {})
        agent.send("session/resume", {"sessionId": session, "cwd": str(AGENT.parent.resolve())}, 6)
        _, replay = agent.response(6)
        self.assertEqual(replay, [])

    def test_delay_does_not_block_cancel_or_other_calls(self):
        agent = self.agent("--delay", "session/prompt=1")
        _, session = agent.start()
        agent.send("session/prompt", {"sessionId": session, "prompt": []}, 3)
        agent.send("session/cancel", {"sessionId": session}, None)
        answer, _ = agent.response(3)
        self.assertEqual(answer["result"]["stopReason"], "cancelled")
        agent.send("session/list", {}, 4)
        self.assertIn("result", agent.response(4)[0])
        with self.assertRaises(queue.Empty):
            agent.next(timeout=1.15)

    def test_hold_and_session_switch_isolation(self):
        agent = self.agent("--hold-prompts")
        _, first = agent.start()
        agent.send("session/prompt", {"sessionId": first, "prompt": []}, "turn")
        agent.send("session/new", {"cwd": str(AGENT.parent.resolve()), "mcpServers": []}, 4)
        opened, _ = agent.response(4)
        second = opened["result"]["sessionId"]
        self.assertNotEqual(first, second)
        agent.send("session/cancel", {"sessionId": second}, None)
        with self.assertRaises(queue.Empty):
            agent.next(timeout=.1)
        agent.send("session/cancel", {"sessionId": first}, None)
        self.assertEqual(agent.response("turn")[0]["result"]["stopReason"], "cancelled")

    def test_refused_setup_and_unknown_method_have_no_side_effects(self):
        agent = self.agent("--refuse", "session/new", "--error-code", "-32000")
        agent.send("initialize", {"protocolVersion": 1})
        agent.response(1)
        agent.send("session/new", {"cwd": "/tmp", "mcpServers": []}, 2)
        self.assertEqual(agent.response(2)[0]["error"]["code"], -32000)
        agent.send("session/list", {}, 3)
        self.assertEqual(agent.response(3)[0]["result"]["sessions"], [])
        agent.send("unknown", {}, 4)
        self.assertEqual(agent.response(4)[0]["error"]["code"], -32601)

    def test_mcp_receipts_and_independent_transport_gate(self):
        agent = self.agent("--mcp-capabilities", "http")
        agent.send("initialize", {"protocolVersion": 1})
        agent.response(1)
        definitions = [{"type": "sse", "name": "events", "url": "https://example.invalid/sse", "headers": []}]
        agent.send("session/new", {"cwd": str(AGENT.parent.resolve()), "mcpServers": definitions}, 2)
        self.assertEqual(agent.response(2)[0]["error"]["code"], -32602)
        definitions[0]["type"] = "http"
        definitions[0]["headers"] = [{"name": "X", "value": ""}, {"name": "X", "value": "two"}]
        agent.send("session/new", {"cwd": str(AGENT.parent.resolve()), "mcpServers": definitions}, 3)
        opened, _ = agent.response(3)
        agent.send("session/prompt", {"sessionId": opened["result"]["sessionId"], "prompt": []}, 4)
        _, updates = agent.response(4)
        receipt = json.loads(updates[-1]["params"]["update"]["content"]["text"])
        self.assertEqual(receipt["mcpServers"], [{"name": "events", "transport": "http", "headers": 2}])

    def test_invalid_base64_and_unadvertised_prompt_are_refused(self):
        agent = self.agent("--prompt-capabilities", "image")
        _, session = agent.start()
        for block in [{"type": "image", "mimeType": "image/png", "data": "!"}, {"type": "audio", "mimeType": "audio/wav", "data": ""}]:
            agent.send("session/prompt", {"sessionId": session, "prompt": [block]}, 3)
            self.assertEqual(agent.response(3)[0]["error"]["code"], -32602)

    def test_delayed_setup_is_scheduled_without_blocking_list(self):
        agent = self.agent("--delay", "session/new=0.3")
        agent.send("initialize", {"protocolVersion": 1})
        agent.response(1)
        agent.send("session/new", {"cwd": str(AGENT.parent.resolve()), "mcpServers": []}, 2)
        agent.send("session/list", {}, 3)
        first = agent.next()
        self.assertEqual(first["id"], 3)
        self.assertEqual(first["result"]["sessions"], [])
        self.assertEqual(agent.next()["id"], 2)

    def test_cli_rejects_unknown_claims_and_unbounded_delays(self):
        for args in [("--prompt-capabilities", "video"), ("--delay", "session/new=nan"),
                     ("--delay", "session/new=31"), ("--delay", "unknown=1")]:
            with self.subTest(args=args):
                result = subprocess.run([sys.executable, str(AGENT), *args], capture_output=True, timeout=3)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(result.stdout, b"")
                self.assertIn(b"error:", result.stderr)

    def test_bad_json_is_reported_and_next_frame_is_processed(self):
        agent = self.agent("--restore", "resume")
        agent.process.stdin.write(b"not json\n")
        agent.process.stdin.flush()
        self.assertEqual(agent.next()["error"]["code"], -32700)
        initialized, session = agent.start()
        caps = initialized["result"]["agentCapabilities"]
        self.assertFalse(caps["loadSession"])
        self.assertIn("resume", caps["sessionCapabilities"])
        agent.send("session/load", {"sessionId": session, "cwd": str(AGENT.parent.resolve())}, 3)
        self.assertEqual(agent.response(3)[0]["error"]["code"], -32602)
        agent.send("session/delete", {"sessionId": session}, 4)
        agent.response(4)
        agent.send("session/list", {}, 5)
        self.assertEqual(agent.response(5)[0]["result"]["sessions"], [])

    def test_delayed_refusal_is_cancellable_with_no_late_error(self):
        agent = self.agent("--refuse", "session/prompt", "--delay", "session/prompt=0.2")
        _, session = agent.start()
        agent.send("session/prompt", {"sessionId": session, "prompt": []}, 3)
        agent.send("session/cancel", {"sessionId": session}, None)
        self.assertEqual(agent.response(3)[0]["result"]["stopReason"], "cancelled")
        with self.assertRaises(queue.Empty):
            agent.next(timeout=.35)
        agent.send("session/prompt", {"sessionId": session, "prompt": []}, 4)
        self.assertEqual(agent.response(4)[0]["error"]["code"], -32602)

    def test_eof_exits_cleanly_with_pending_work(self):
        for args in [("--hold-prompts",), ("--delay", "session/prompt=30")]:
            with self.subTest(args=args):
                agent = self.agent(*args)
                _, session = agent.start()
                agent.send("session/prompt", {"sessionId": session, "prompt": []}, 3)
                # A list reply establishes the held/delayed prompt was processed.
                agent.send("session/list", {}, 4)
                agent.response(4)
                agent.process.stdin.close()
                self.assertEqual(agent.process.wait(timeout=2), 0)
                agent.diagnostics.join(timeout=2)
                self.assertEqual(agent.errors, [])

    def test_close_and_delete_cancel_independent_pending_turns(self):
        agent = self.agent("--hold-prompts")
        _, first = agent.start()
        agent.send("session/new", {"cwd": str(AGENT.parent.resolve())}, 3)
        second = agent.response(3)[0]["result"]["sessionId"]
        for sid, ident in [(first, 4), (second, 5)]:
            agent.send("session/prompt", {"sessionId": sid, "prompt": []}, ident)
        agent.send("session/close", {"sessionId": first}, 6)
        self.assertEqual(agent.next(), {"jsonrpc": "2.0", "id": 4, "result": {"stopReason": "cancelled"}})
        self.assertEqual(agent.next()["id"], 6)
        agent.send("session/delete", {"sessionId": second}, 7)
        self.assertEqual(agent.next(), {"jsonrpc": "2.0", "id": 5, "result": {"stopReason": "cancelled"}})
        self.assertEqual(agent.next()["id"], 7)
        agent.send("session/list", {}, 8)
        self.assertEqual([s["sessionId"] for s in agent.response(8)[0]["result"]["sessions"]], [first])

    def test_mcp_replacement_and_two_session_histories(self):
        agent = self.agent()
        _, first = agent.start()
        cwd = str(AGENT.parent.resolve())
        definitions = [
            {"name": "local", "command": sys.executable, "args": ["", "two"], "env": [{"name": "X", "value": ""}]},
            {"type": "sse", "name": "events", "url": "https://example.invalid/sse", "headers": []},
        ]
        agent.send("session/new", {"cwd": cwd, "mcpServers": definitions}, 3)
        second = agent.response(3)[0]["result"]["sessionId"]
        histories = {}
        for sid, ident, text in [(first, 4, "first"), (second, 5, "second")]:
            agent.send("session/prompt", {"sessionId": sid, "prompt": [{"type": "text", "text": text}]}, ident)
            _, histories[sid] = agent.response(ident)
        receipt = json.loads(histories[second][0]["params"]["update"]["content"]["text"])
        self.assertEqual(receipt["mcpServers"], [{"name": "local", "transport": "stdio", "arguments": 2, "environment": 1}, {"name": "events", "transport": "sse", "headers": 0}])
        for sid, ident in [(first, 6), (second, 7)]:
            agent.send("session/load", {"sessionId": sid, "cwd": cwd, "mcpServers": []}, ident)
            self.assertEqual(agent.response(ident)[1], histories[sid])
        agent.send("session/prompt", {"sessionId": second, "prompt": []}, "after-load")
        receipt = json.loads(agent.response("after-load")[1][0]["params"]["update"]["content"]["text"])
        self.assertEqual(receipt["mcpServers"], [])
        agent.send("session/resume", {"sessionId": second, "cwd": cwd, "mcpServers": definitions[1:]}, 8)
        self.assertEqual(agent.response(8)[1], [])
        agent.send("session/prompt", {"sessionId": second, "prompt": []}, 9)
        receipt = json.loads(agent.response(9)[1][0]["params"]["update"]["content"]["text"])
        self.assertEqual(receipt["mcpServers"], [{"name": "events", "transport": "sse", "headers": 0}])


if __name__ == "__main__":
    unittest.main()
