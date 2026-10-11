"""An agent that reads everything and answers nothing: the shape `--timeout` is for.

It exits when its stdin closes, which is the first step of the CLI's graceful stop.
"""
import sys

for _ in sys.stdin:
    pass
