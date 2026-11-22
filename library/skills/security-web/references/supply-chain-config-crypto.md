# Supply chain, configuration, cryptography and secrets

Read from the `security-web` skill. Covers A02:2025 Security Misconfiguration, A03:2025 Software Supply Chain Failures, A04:2025 Cryptographic Failures, A08:2025 Software or Data Integrity Failures, and ASVS 5.0 chapters V11 Cryptography, V12 Secure Communication, V13 Configuration, V14 Data Protection, V15 Secure Coding and Architecture. OWASP wording is not reproduced; open the linked chapter for a requirement's exact text.

## A new or upgraded dependency

Report every dependency the change adds or upgrades, with what you checked:

- It exists under that exact name on the registry and is the project you mean: models invent plausible package names, and attackers register them; one letter off is a typosquat.
- Publisher, source repository link, age, release history, maintainers: a package published last week by a new account for a common task is a red flag.
- Install-time code: `preinstall`/`postinstall` scripts in npm, build scripts, setup hooks. Flag any that download or execute.
- Its licence fits the project.
- The lockfile changed with it and is committed; CI installs from the lockfile only (`npm ci`, `pnpm install --frozen-lockfile`, `yarn install --immutable`, `uv sync --locked`, `poetry install` with the lock, `cargo build --locked`, `go mod verify`, `composer install`, `bundle install` with `BUNDLE_FROZEN`).
- Prefer the standard library or a dependency the project already has; a few lines of code beat a new package.

## Build and integrity (A03, A08)

- No `curl … | sh` or download-and-run in builds; download with a pinned checksum or signature, and verify it.
- Release artifacts and auto-update channels are signed and verified before use.
- CI workflow hardening has its own skill, `github-actions`.
- Data the server signs (cookies, state, links) is verified with a constant-time comparison before it is trusted.

## Configuration (A02)

- Debug modes, stack traces in responses, directory listings, default accounts, sample apps and admin consoles are off or removed outside development.
- Security headers on HTML responses: `Strict-Transport-Security`, `Content-Security-Policy` (with `frame-ancestors`), `X-Content-Type-Options: nosniff`, `Referrer-Policy`. Use the framework's middleware for them.
- Cookies: `Secure`, `HttpOnly` unless JavaScript must read them, `SameSite`.
- New configuration keys default to the safe value; an insecure option has to be turned on explicitly.
- Storage buckets, queues and databases are private and reachable only from the services that use them.

## Cryptography (A04)

- TLS for every connection that leaves the host, with certificate verification on (no `verify=False`, `rejectUnauthorized: false`, `InsecureSkipVerify`).
- Use the platform's vetted primitives: AES-GCM or ChaCha20-Poly1305 for encryption, Argon2id, scrypt or bcrypt for passwords, HMAC-SHA-256 for signatures. No home-made schemes, no ECB, no MD5 or SHA-1 for anything security related.
- Random values for tokens, ids and keys come from a CSPRNG (`secrets`, `crypto.randomBytes` / `crypto.getRandomValues`, `crypto/rand`, `SecureRandom`), never `Math.random` or `random`.
- Compare secrets and MACs in constant time (`hmac.compare_digest`, `crypto.timingSafeEqual`, `subtle.ConstantTimeCompare`).

## Secrets and personal data (V14)

- No secret in code, tests, fixtures, examples or commit messages; read them from the environment or the secret manager the project uses. `.env` files stay out of git.
- A secret that reached git must be rotated: removing it from the tip leaves it in history.
- Keep tokens, passwords and personal data out of logs, URLs and error messages; mask them in structured logs.
- Store only the personal data the feature needs, and say in your report what new personal data the change stores.

## Links

- A02, A03, A04, A08 (2025): https://top10.owasp.org/2025
- ASVS 5.0 chapters V11 to V15: https://github.com/OWASP/ASVS/tree/master/5.0/en
- OWASP Cheat Sheets (Secrets Management, Cryptographic Storage, Password Storage, HTTP Headers, Software Supply Chain): https://cheatsheetseries.owasp.org/
