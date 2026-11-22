---
name: redis
description: >-
  Uses Redis or Valkey from application code the safe way: caching with TTLs, key naming, choosing the data
  structure, counters, rate limits, locks, queues and streams, pipelines, cluster hash slots, memory and
  eviction, and the licence question when adding a Redis server. Use when a task touches a Redis or Valkey
  client (redis, ioredis, redis-py, go-redis, Jedis, Lettuce, Predis), REDIS_URL, redis.conf or redis-cli.
  Not for SQL databases (sql-postgres, mysql) nor MongoDB (mongodb).
license: MIT
metadata:
  termf-stacks: "redis valkey"
  termf-roles: "back devops dba"
  termf-versions: "*"
  termf-verified: "2026-11-22 redis@8.10.3 valkey@9.2.0 redis/agent-skills@main valkey-io/valkey-skills@main"
  termf-sources: "https://redis.io/llms.txt https://redis.io/legal/licenses/ https://redis.io/docs/latest/operate/oss_and_stack/stack-with-enterprise/release-notes/redisce/redisos-8.0-release-notes/ https://github.com/redis/agent-skills https://github.com/valkey-io/valkey-skills https://github.com/valkey-io/valkey/releases"
---
# Redis and Valkey

Redis publishes official skills in `redis/agent-skills` (MIT): `redis-core` (data structures and key names), `redis-connections`, `redis-security`, `redis-clustering`, `redis-observability`, `redis-search`, `redis-semantic-cache`. Valkey publishes `valkey` in `valkey-io/valkey-skills` (BSD-3-Clause). This skill stays thin: the traps, the security defaults and the checks. The Redis documentation is CC BY-NC-SA 4.0, so nothing here is taken from it: facts are restated and linked.

1. Read which server and version the project runs: the image in Compose or CI (`redis:8`, `valkey/valkey:9`), the managed service, or `redis-cli INFO server`. Read the client library and its version in the lockfile. Done when you can name server, version and client.
2. If `redis-core` or `valkey` is loaded in this session, follow it for data modelling and key naming, and keep this skill for the traps and checks below. If not, carry on here and name the matching repository in your report so the user can import it. Done when you know which guidance you follow.
3. Reuse the project's client wrapper, key prefix and serializer, because two key schemes in one database collide and cannot be cleaned up. Done when every key you write follows the existing prefix.
4. Give every cache key a TTL, set atomically with the value (`SET key value EX 300`, or `NX` for a lock), and document who invalidates it. Done when no new code path writes a cache key without an expiry.
5. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- `KEYS`, `FLUSHALL`, `FLUSHDB` and whole-collection reads (`HGETALL`, `SMEMBERS`, `LRANGE 0 -1`) on large keys block the single command thread for every client. Iterate with `SCAN`, `HSCAN`, `SSCAN` and a `COUNT`.
- Redis 8 ships the former Redis Stack features in the server: JSON, the query engine (`FT.*`), time series, probabilistic structures and vector sets. Code using them does not run on Redis 7 without modules, nor on Valkey, which has its own modules. Check the server before using any `JSON.*` or `FT.*` command.
- Redis 8 puts those commands into the existing ACL categories: `+@read` now allows `FT.SEARCH`, and `+@all -@write` now blocks `JSON.SET`. Recheck custom ACL users on upgrade.
- Hash fields can expire on their own since Redis 7.4 (`HEXPIRE`), and Redis 8.0 adds `HGETEX`, `HSETEX`, `HGETDEL`. On Valkey or an older server, check first with `COMMAND INFO HGETEX`.
- In a cluster, a multi-key command or a transaction only works when all keys hash to one slot: put the shared part in braces (`cart:{42}:items`, `cart:{42}:total`).
- `MULTI`/`EXEC` does not roll back on a failed command; for a read-then-write that must be atomic, use a Lua script or a function.
- Licence: Redis 8 and later is under your choice of AGPLv3, RSALv2 or SSPLv1; 7.4 to 7.8 under RSALv2 or SSPLv1; 7.2 and earlier BSD-3-Clause. Valkey (the Linux Foundation fork) is BSD-3-Clause. When a task adds or upgrades a Redis server, state the licence in your report; client libraries have their own licences.
- `maxmemory-policy` defaults to `noeviction`: a full cache then rejects writes. A pure cache wants `allkeys-lru` or `allkeys-lfu`; a store of record keeps `noeviction`.

## Verify

- The project's tests first, against the local Redis or Valkey from Compose (in a TermF worktree its port is in `TERMF_PORT_<SERVICE>`).
- `redis-cli -u "$REDIS_URL" PING` (or `valkey-cli`).
- `redis-cli -u "$REDIS_URL" --scan --pattern '<prefix>:*' | head` and `TTL <key>` on a key your code wrote.
- `redis-cli -u "$REDIS_URL" --bigkeys` after a change that grows a key.

## Security

- Never expose the port publicly: bind to localhost or a private network, keep `protected-mode yes`, and require authentication: an ACL user for the application, and the `default` user disabled or given a strong password.
- Give the application its own ACL user with only its key prefix and the command categories it uses (`-@dangerous`), so a compromised app cannot run `CONFIG`, `FLUSHALL` or `DEBUG`.
- Build Lua scripts as fixed text with `KEYS` and `ARGV`, never by concatenating input; keep the server patched (the Lua sandbox had remote code execution fixes in October 2025).
- Do not store secrets or personal data in a cache without a TTL, and use TLS (`rediss://`) outside a private network.

## References

- Official skills: https://github.com/redis/agent-skills (`/plugin install redis-development@claude-plugins-official` or `npx skills add redis/agent-skills`) ; Valkey: https://github.com/valkey-io/valkey-skills (`npx skills add valkey-io/valkey-skills`).
- Licences per version: https://redis.io/legal/licenses/
