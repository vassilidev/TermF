---
title: Server health
order: 8
---

One click on an SSH connection reads how a server is doing: CPU, memory, disks, services, ports and SSL certificates. It only reads.

![The health of a production server: CPU and load, memory, two disks, three SSL certificates with their expiry, the running services and the listening ports](shot:server "A server's health, read-only. Sample data.")

## Read it {#read}

Pick an SSH connection in **Ops › Connections**, then **Read** in the **Server health** panel. **Read again** refreshes it.

| What | How it reads |
|---|---|
| CPU | Its use over one second, and the load for the number of cores |
| Memory | Used of total |
| Disks | Each mounted disk, filled from 0 to 100 % |
| SSL certificates | Each site Nginx serves, with the days before its certificate expires: soon in orange, expired in red |
| Services | The services running |
| Ports | The ports listening |

The `server-health` recipe runs it. It changes nothing on the server and reads no key file: the certificates are read as Nginx serves them.

## With an agent {#agents}

An agent reads the same through the recipe, after your approval, like any Ops recipe. Ask from <kbd>⌘I</kbd> on Ops, for example "why is acme-prod slow?". See [Servers, databases and logs](ops.md).

> [!NOTE]
> The certificates are read on the server's own port 443, for at most 30 site names of Nginx's enabled sites.
>
> **Tried so far.** Never against a real server: built from the known outputs of `vmstat`, `free`, `df`, `systemctl`, `ss` and `openssl`, and checked with automated tests and a simulated backend.
