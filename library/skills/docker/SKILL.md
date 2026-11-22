---
name: docker
description: >-
  Writes and fixes Dockerfiles and Compose files the way current Docker Engine and Compose expect: smaller
  and safer images, build secrets, non-root users, multi-stage builds, healthchecks, services that wait for
  their database, local stacks for a worktree. Use when a task touches a Dockerfile, a compose.yaml or
  docker-compose.yml, a .dockerignore, or docker build and docker compose commands. Not for Kubernetes
  manifests or Helm charts (kubernetes), nor for CI workflow files (github-actions).
paths: "**/Dockerfile, **/Dockerfile.*, **/*.Dockerfile, **/compose.y*ml, **/compose.*.y*ml, **/docker-compose*.y*ml, **/.dockerignore"
license: MIT
metadata:
  termf-stacks: "docker compose"
  termf-roles: "devops back"
  termf-versions: ">=25"
  termf-verified: "2026-11-22 docker-engine@29.10.0 compose@5.8.0 docker/skills@main"
  termf-sources: "https://docs.docker.com/llms.txt https://docs.docker.com/engine/release-notes/29/ https://github.com/docker/compose/releases https://docs.docker.com/build/checks/ https://docs.docker.com/reference/compose-file/version-and-name/ https://github.com/docker/skills"
---
# Docker and Compose

Docker publishes its own skills in `docker/skills` (Apache-2.0): `docker-build-strategies`, `docker-compose-patterns`, `docker-destructive-guardrails`, `docker-project-foundations`. This skill stays thin: it keeps the traps of the current versions, the security defaults and the checks.

1. Read the versions in play: `docker version --format '{{.Server.Version}}'`, `docker compose version`, and the `# syntax=` line at the top of the Dockerfile. Done when you can name the Engine and Compose versions, or know that Docker is not installed here.
2. If `docker-build-strategies` or `docker-compose-patterns` is loaded in this session, follow it for image and Compose patterns and keep this skill for the gotchas and checks below. If not, carry on here and name `docker/skills` in your report so the user can import it. Done when you know which guidance you follow.
3. Keep the files and names the repository already uses (`compose.yaml` or `docker-compose.yml`, base image family, registry), because a second convention splits the project. Done when every file you touch already existed or the task asked for it.
4. For a Dockerfile: multi-stage build with a slim final stage, dependency manifests copied before the source so the install layer stays cached, a non-root `USER`, a base image pinned by tag and digest, build-time secrets through `RUN --mount=type=secret`. Done when `docker build --check .` exits 0.
5. For Compose: no top-level `version:`, a `healthcheck` on each service others depend on, and `depends_on` with `condition: service_healthy`. Done when `docker compose config -q` exits 0.
6. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- Compose v5 (December 2025) removed its internal builder: `docker compose build` goes through Bake and BuildKit, like `docker build`, so a build that works with one works with the other. The legacy `docker-compose` v1 binary is gone; write `docker compose`.
- The top-level `version:` key is obsolete and only triggers a warning; Compose validates against the latest schema whatever it says. Delete it instead of bumping it.
- `depends_on: [db]` only orders start-up; the app still races a database that is not ready. Add a healthcheck to `db` and `condition: service_healthy` on the dependent service.
- `docker build --check .` runs the Dockerfile checks without building and exits non-zero on any violation (Dockerfile frontend 1.8+, Buildx 0.15+). `# check=error=true` under `# syntax=docker/dockerfile:1` makes a normal build fail on them too.
- `ARG` and `ENV` values stay in the image metadata and history, so a token passed that way ships with the image (the check `SecretsUsedInArgOrEnv` flags it). Use `RUN --mount=type=secret,id=npm_token` and `docker build --secret id=npm_token,env=NPM_TOKEN`.
- Engine 29 makes the containerd image store the default on fresh installs and removed Docker Content Trust from the CLI: `DOCKER_CONTENT_TRUST=1` no longer verifies anything, so pin images by digest instead.
- In a TermF worktree, `COMPOSE_PROJECT_NAME`, `PORT` and `TERMF_PORT_<SERVICE>` are set so parallel worktrees do not share containers or ports. Use them from the environment (`"127.0.0.1:${TERMF_PORT_DB:-5432}:5432"`, only if the task asks to change the file) and keep `-p` and hard-coded host ports out of your commands.

## Verify

- The project's own targets first (`Makefile`, `justfile`, `package.json` scripts that wrap Docker).
- `docker build --check .` then `docker build -t <name>:check .`
- `docker compose config -q`
- `hadolint Dockerfile` when hadolint is installed.

## Security

- Run as a non-root user with a fixed UID (`USER 10001`); never add `--privileged` or mount `/var/run/docker.sock` into a container, since either gives root on the host.
- Keep `.env*`, `.git`, keys and local data out of the build context with `.dockerignore`; a `COPY . .` sends everything that is not excluded.
- Published ports bypass host firewalls such as ufw on Linux; bind development databases and caches to `127.0.0.1:` in Compose.
- `docker compose down -v`, `docker volume rm` and `docker system prune -a` delete data volumes, including a local database. Ask before any of them; Docker's `docker-destructive-guardrails` skill covers the full list.

## References

- Docs index for agents: https://docs.docker.com/llms.txt (pages also exist as Markdown).
- Build checks and their rule names: https://docs.docker.com/build/checks/
- Official skills: https://github.com/docker/skills (`npx skills add docker/skills`).
