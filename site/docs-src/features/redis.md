---
title: Redis
order: 8
---

A Redis connection in Ops shows the server's numbers and lets you find a key and read its value. Changes wait for your confirmation.

![A Redis connection in Ops: version, memory, clients, keys and hit rate, the keys found by a pattern, and a hash key's fields](shot:redis "The keys of a Redis server and a hash's fields. Sample data.")

## Connect {#connect}

1. In **Ops**, **Add a connection** of kind Redis: its host and port, its database number (0 by default) and its environment.
2. A Redis server behind a server is reached through an SSH connection of the project, like a database.
3. The password goes to the [vault](vault.md). TermF hands it to `redis-cli` through its environment, never on the command line.
4. **Test** it.

## Read {#read}

Pick the connection in **Connections**:

- the top line reads `INFO`: version, memory used, clients, keys and hit rate;
- **Pattern** and **Search** list the keys that match, such as `user:*`, a page at a time with `SCAN` (never `KEYS`, which blocks the server); **Next page** goes on;
- a click on a key shows its type, its time to live and its value, laid out by type: string, hash, list, set, sorted set or stream, the first 200 items.

Reads go through the `redis-read` recipe, an allow list of read commands only. Values come back masked like any Ops output.

## Change {#change}

**Delete** a key, **Expire** it in some seconds, **Remove the expiry**, or **Empty the database**. A card says what changes, then asks you to type the environment's name.

These go through the `redis-write` recipe: `DEL`, `UNLINK`, `EXPIRE`, `PERSIST`, `FLUSHDB` and `FLUSHALL`, nothing else.

An agent asks for the same changes through its own card, see [Agents that act for you](agent-actions.md), and reads through the recipes, after your approval.

> [!NOTE]
> A value on several lines inside a hash or a list reads poorly: `redis-cli` gives raw output. A string reads in full.
>
> **Tried so far.** Never against a real Redis: built from its documentation and checked with automated tests and a simulated backend.
