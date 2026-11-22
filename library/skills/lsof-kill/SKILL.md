---
name: lsof-kill
description: >-
  Starts, finds and stops local processes from an agent's shell without touching anyone else's: running a dev
  server or a watcher in the background, finding what listens on a port, stopping exactly the process you
  started by its PID, dealing with a port already in use, and leaving nothing running at the end. Use when a
  task starts a server, hits "address already in use" or EADDRINUSE, or asks to kill or restart a process. Not
  for containers (docker), nor for processes on another machine.
license: MIT
metadata:
  termf-stacks: "*"
  termf-roles: "all"
  termf-versions: "*"
  termf-verified: "2026-11-22 lsof@4.99.7, Claude Code tools reference, Windows netstat and taskkill"
  termf-sources: "https://lsof.readthedocs.io/en/latest/manpage/ https://pubs.opengroup.org/onlinepubs/9799919799/utilities/kill.html https://code.claude.com/docs/en/tools-reference https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/netstat https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/taskkill"
---
# lsof and kill

1. Use the port TermF gave this worktree (`$PORT`, `$TERMF_PORT_<SERVICE>`) and pass it to the server the way the project reads it (`--port "$PORT"`, a `PORT` variable), because the default port may belong to another worktree or to the user. Done when the command names its port.
2. Start a long-running process (a dev server, a watcher) with the Bash tool's background option (`run_in_background`) rather than in the foreground, where it blocks until the timeout. Claude Code then tracks it as a task you can stop. Wait for it to be ready with `curl --retry-connrefused` (skill `curl`), not a fixed `sleep`. Done when it answers and you have its task id or PID.
3. Before acting on a busy port, find out whose process holds it: `lsof -nP -iTCP:<port> -sTCP:LISTEN` gives the PID, the command and the user, and `ps -o pid,ppid,user,command -p <pid>` the full command line. On Windows, `netstat -ano` lists the listening ports with the PID in the last column. Done when you can say whether you started that process in this session.
4. Stop only what you started: the background task with the tool that stops tasks (TaskStop), or `kill <pid>` for a PID you started, then `kill -9 <pid>` only if it is still there a few seconds later; on Windows, `taskkill /pid <pid>`. A port held by anything else (the user's server, another worktree, TermF's own preparation) stays as it is: use another free port or report the PID, command and user. Done when every signal you sent went to a process you started.
5. Before you report, stop everything you started, or say what is still running and why. Done when `lsof -nP -iTCP:<port> -sTCP:LISTEN` shows nothing of yours.
6. Run the commands of `## Verify` and put each one with its result in your report.

## Gotchas

- Killing by name ends other people's processes: `pkill node`, `pkill -f server`, `killall python` and `taskkill /im node.exe` also hit the user's editor and tools, the servers of other worktrees, and other agent sessions. `pkill` and `pgrep` match a pattern anywhere in the process name, or anywhere in the whole command line with `-f`, so `pgrep -l slee` already lists every `sleep` of every session. Kill by PID only.
- In a headless session (`claude -p`, which TermF team steps use), a background command started by the main conversation keeps the run open after its answer until the command exits or reaches its time limit (30 minutes by default). Stop it before you answer.
- A foreground command that reaches its timeout is moved to the background, not stopped: a server started in the foreground keeps running and holding its port.
- `lsof -t` prints only PIDs, which is handy for `kill`, but `lsof -ti :3000` also lists processes that are merely connected to that port (a browser, a client): add `-sTCP:LISTEN` with `-iTCP:<port>` to get only the server.
- `lsof -sTCP:LISTEN` needs `-i` too; `-n` and `-P` skip name lookups, which otherwise make it slow.
- A process killed with `-9` gets no chance to clean up (temporary files, child processes, a lock file): send the default signal first.

## Verify

- `lsof -nP -iTCP:<port> -sTCP:LISTEN` (Windows: `netstat -ano`) shows the server while you test, and nothing of yours once you are done.
- The background tasks you started are all stopped, or named in your report with the reason they still run.

## Security

- A process you did not start may hold the user's work (an unsaved editor, a database, another agent's run): ask before signalling it, even when it blocks your port.
- Never run `sudo` to free a port or kill a process: a process owned by another user is not yours to stop.
- Bind development servers to `127.0.0.1` when the project lets you choose, so the worktree's server is not reachable from the network.

## References

- lsof manual (selection options `-i`, `-s`, `-t`): https://lsof.readthedocs.io/en/latest/manpage/
- Background commands in Claude Code and their time limit: https://code.claude.com/docs/en/tools-reference
