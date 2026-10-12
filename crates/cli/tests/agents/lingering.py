"""An agent that will not go: it answers nothing, ignores SIGTERM, and outlives its stdin.

Only SIGKILL ends it, which is what the CLI's stop comes to. It writes its pid to
the file named by its one argument, so a test can wait for it to start and check
it is gone afterwards. Standard library only.
"""
import os
import signal
import sys
import time

signal.signal(signal.SIGTERM, signal.SIG_IGN)
signal.signal(signal.SIGINT, signal.SIG_IGN)
with open(sys.argv[1] + ".partial", "w") as pidfile:
    pidfile.write(str(os.getpid()))
os.rename(sys.argv[1] + ".partial", sys.argv[1])

for _ in sys.stdin:
    pass
while True:
    time.sleep(60)
