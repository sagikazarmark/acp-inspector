#!/usr/bin/env python3
"""#5 fixture: Testy's callbacks with literal numeric defaults on the wire.

Run as the Agent command (python3, with this absolute path as its argument).
Only Testy's form schema is modified; all other traffic passes through.
"""

import pathlib
import subprocess
import sys
import threading

testy = pathlib.Path(__file__).resolve().parents[1] / ".testy/bin/testy"
agent = subprocess.Popen([str(testy)], stdin=subprocess.PIPE, stdout=subprocess.PIPE)


def forward_input():
    for line in sys.stdin.buffer:
        agent.stdin.write(line)
        agent.stdin.flush()
    agent.stdin.close()


threading.Thread(target=forward_input, daemon=True).start()
try:
    for line in agent.stdout:
        if b'"requestedSchema"' in line:
            # Token substitution, not a parse/print round trip: the fixture
            # deliberately contains spellings serde_json's typed f64 loses.
            line = line.replace(
                b'"confidence":{"type":"number","minimum":0.0,"maximum":1.0}',
                b'"confidence":{"type":"number","minimum":0.0,"maximum":1e2,"default":1.50}',
            )
        sys.stdout.buffer.write(line)
        sys.stdout.buffer.flush()
finally:
    agent.terminate()
    agent.wait(timeout=5)
