---
title: Nginx
order: 8
---

On an SSH connection, Ops tells you whether Nginx runs and whether its configuration is valid, follows its logs live, and reloads it only when the test passes.

![Ops on a production server: the server's health, then Nginx running, its configuration valid, its enabled sites, and the buttons to follow its logs or reload it](shot:server "Nginx and the server's health on an SSH connection. Sample data.")

## Check it {#status}

Pick an SSH connection in **Ops › Connections**, then **Read** in the **Nginx** panel:

- **Running**, or stopped with its state;
- what `nginx -t` says: the configuration is valid, or the error with its messages;
- the enabled sites.

## Follow its logs {#logs}

**Live access** and **Live errors** open the **Live** tab on Nginx's access and error logs.

- An access line takes the level of its status: a 5xx is an error, a 4xx a warning. The **Errors** filter shows the 5xx.
- The text filter finds an IP address or a URL.

See [Live logs](ops.md#live-logs) for pause, clear and stop.

## Reload it {#reload}

**Reload** is offered once `nginx -t` passed. A card says what changes, then asks you to type the environment's name. The `nginx-reload` recipe runs `nginx -t` again on the server, and reloads only if it passes: open connections are not dropped.

An agent can ask for the same reload through its card. See [Agents that act for you](agent-actions.md).

> [!NOTE]
> The full `nginx -t` and the reload need `sudo` without a password for `nginx` and `systemctl reload nginx` on the server. Without it, the status tries `nginx -t` without sudo.
>
> **Tried so far.** Never against a real Nginx: built from known outputs and checked with automated tests and a simulated backend.
