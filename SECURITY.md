# Security policy

## Reporting a vulnerability

Please report security issues privately, through GitHub's private vulnerability
reporting:
[**Report a vulnerability**](https://github.com/sagikazarmark/acp-inspector/security/advisories/new).
Do not open a public issue for them.

Include what you found, how to reproduce it (an agent script or a trace export
helps), and the version or commit. You will get an answer as soon as the report
has been read; a fix and an advisory follow once the issue is confirmed.

## Supported versions

Only the latest release gets security fixes.

## What is in scope

The inspector spawns an agent you name and runs it with your privileges, so an
agent being able to act as you is not a vulnerability: that is what running it
means. The inspector does not sandbox agents, and it declines `fs/*` and
`terminal/*` requests rather than mediating them, so agents do their own file
I/O and run their own commands directly
([Known limitations](README.md#known-limitations)).

In scope is anything where the inspector itself does more than you asked, for
example:

- traffic from an agent making the inspector run code, open a URL or a file, or
  write outside its own state and export files without your action;
- the window rendering agent-supplied content as active markup or script;
- secrets leaking beyond the files and exports the README says they are in.
