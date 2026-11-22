---
name: mongodb
description: >-
  Models and queries MongoDB data the way the installed server expects: embedding or referencing, indexes and
  explain plans, aggregation pipelines, transactions, schema validation, Mongoose and driver code, and the
  MongoDB 9.0 changes. Use when a task touches a MongoDB driver or Mongoose, MONGODB_URI or mongodb+srv://,
  mongosh, aggregation pipelines or index definitions. Not for SQL databases (sql-postgres, mysql) nor Redis
  (redis).
license: MIT
metadata:
  termf-stacks: "mongodb"
  termf-roles: "back dba"
  termf-versions: "*"
  termf-verified: "2026-11-22 mongodb@9.0.8 mongodb-mcp-server@3.3.0 mongodb/agent-skills@main"
  termf-sources: "https://www.mongodb.com/docs/llms.txt https://www.mongodb.com/docs/manual/release-notes/9.0/ https://github.com/mongodb/agent-skills https://github.com/mongodb-js/mongodb-mcp-server"
---
# MongoDB

MongoDB publishes official skills in `mongodb/agent-skills` (Apache-2.0): `mongodb-schema-design`, `mongodb-query-optimizer`, `mongodb-connection`, `mongodb-mcp-setup`, `mongodb-natural-language-querying`, `mongodb-search-and-ai`. This skill stays thin: the 9.0 traps, the security defaults and the checks. The MongoDB documentation is not under an open licence, so nothing here is taken from it.

1. Read the server version (the `mongo:<v>` image in Compose or CI, the Atlas cluster, or `db.version()`), the feature compatibility version (`db.adminCommand({getParameter: 1, featureCompatibilityVersion: 1})`), and the driver or Mongoose version in the lockfile. Done when you can name all three.
2. If `mongodb-schema-design` or `mongodb-query-optimizer` is loaded in this session, follow it for modelling and indexing, and keep this skill for the traps and checks below. If not, carry on here and name `mongodb/agent-skills` in your report so the user can import it. Done when you know which guidance you follow.
3. Model from the reads: embed what is always read with its parent and stays bounded, reference what grows without limit or is shared, since a document is capped at 16 MB and an unbounded array slows every write. Done when each new field or collection has its main query written down.
4. Index each new query: compound keys in the order equality, then sort, then range, and confirm with `explain("executionStats")` that the plan is an `IXSCAN` and `totalDocsExamined` stays close to `nReturned`. Done when every new query has such a plan.
5. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- MongoDB 9.0 (2026-09-28, latest 9.0.8) caps the memory of a single operation (`internalQueryMaxMemoryUsageBytesPerOperation`, by default the larger of 1 GB and 20 % of RAM): an aggregation that used to pass can now fail with `ExceededMemoryLimit` or `QueryExceededMemoryLimitNoDiskUseAllowed`. Filter and project earlier, index the `$match` and `$sort`, then `allowDiskUse: true` if still needed.
- 9.0 validates all incoming BSON by default: a driver or import that sends invalid BSON (bad UTF-8, for instance) now gets an error instead of a stored document.
- At feature compatibility version 9.0, `listIndexes` reports simple-collation indexes with `collation: { locale: "simple" }`, which makes Mongoose `syncIndexes()` drop and rebuild unchanged indexes. Use Mongoose 9.10.4 or 8.24.5 or later before calling it; Doctrine ODM and Beanie can be affected too.
- `$where`, `$function` and `$accumulator` are no longer deprecated in 9.0 (they run on a WebAssembly JavaScript engine, not on ppc64le); they are still slow and run code, so keep to aggregation operators.
- 9.0 adds the `constraint` validation level, which checks every existing document as well as every write; `moderate` and `strict` do not check documents already stored.
- Multi-document transactions need a replica set, even locally (start `mongod --replSet rs0` and run `rs.initiate()`), and abort after 60 seconds by default. Keep them short; most single-document updates do not need one.
- In a TermF worktree, use the database the worktree's env files name; a remote or Atlas production URI is never yours to touch.

## Verify

- The project's tests first, against the local server from Compose.
- `mongosh "$MONGODB_URI" --quiet --eval 'db.runCommand({ping: 1})'`
- `mongosh "$MONGODB_URI" --quiet --eval 'db.<coll>.find(<filter>).sort(<sort>).explain("executionStats").executionStats'` for each new query.
- `mongosh "$MONGODB_URI" --quiet --eval 'db.<coll>.getIndexes()'` after an index change.

## Security

- Request bodies can carry operators: `{ "password": { "$ne": null } }` matches any user. Validate types at the boundary and keep untrusted objects out of filters (Mongoose `sanitizeFilter`, or cast to string).
- Never pass user input to `$where`, `$function`, `$accumulator` or `mapReduce`.
- Connect with a user that has a role scoped to the application's database, and keep the URI with its password in the environment, never in code or logs.
- The official MCP server writes by default; when one is configured for exploration, run it read-only (`--readOnly` or `MDB_MCP_READ_ONLY=true`).

## References

- Official skills: https://github.com/mongodb/agent-skills (`/plugin install mongodb-atlas`, or `npx skills add mongodb/agent-skills`).
- 9.0 compatibility changes: https://www.mongodb.com/docs/manual/release-notes/9.0/ ; docs index for agents: https://www.mongodb.com/docs/llms.txt
