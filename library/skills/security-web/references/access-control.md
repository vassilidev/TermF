# Access control, authentication, sessions and tokens

Read from the `security-web` skill. Covers A01:2025 Broken Access Control (SSRF included), A07:2025 Authentication Failures, and ASVS 5.0 chapters V6 Authentication, V7 Session Management, V8 Authorization, V9 Self-contained Tokens, V10 OAuth and OIDC. OWASP wording is not reproduced; open the linked chapter for a requirement's exact text.

## Authorization

- Scope every query by id to the principal: `WHERE id = $1 AND account_id = $2`, or the ORM's scoped relation (`current_user.orders.find(id)`), in reads, updates, deletes, exports, downloads, signed URL generation, background jobs that act for a user, and each GraphQL resolver that loads by id. A missing scope is the most frequent real finding.
- Answer 404 rather than 403 when the existence of the object is itself private.
- Check roles on the server for every admin or privileged action; a hidden button or route is not a control.
- Bind request bodies to an explicit list of writable fields. Fields such as `role`, `is_admin`, `account_id`, `owner_id`, `price` or `status` reach the model only through code that checks the caller may set them (mass assignment).
- Repeat the check where the data is read, even when a middleware, proxy or gateway already filters the route.
- Test with two users in two tenants: each attempt across the boundary gets 403 or 404, including through list endpoints with filters and through bulk operations.

## Outgoing requests to URLs from users (SSRF, part of A01:2025)

- Prefer an allowlist of hosts. Otherwise resolve the name, refuse loopback, private, link-local (cloud metadata at `169.254.169.254`) and unique-local addresses, connect to the address you checked, and check again on every redirect or disable redirects.
- Allow only `https` (and `http` if needed); refuse `file:`, `gopher:`, `ftp:` and the like.
- Do not return the raw response body or error of the fetch to the caller.

## Cross-origin requests

- Cookie-authenticated requests that change state need the framework's CSRF protection, or `SameSite=Lax` or `Strict` cookies plus a check of the `Origin` header; a `GET` never changes state.
- CORS: an explicit allowlist of origins; never reflect the request's `Origin` with `Access-Control-Allow-Credentials: true`.

## Authentication

- Hash passwords with the framework's hasher (Argon2id, scrypt or bcrypt as configured); never a fast hash, never a custom scheme.
- Rate-limit or slow down login, password reset, MFA and invitation endpoints per account and per source.
- Reset and invitation tokens: random from a CSPRNG, single use, short lived, stored hashed, invalidated when the password changes.
- Give the same answer and timing for "unknown account" and "wrong password" where the product allows it.
- Re-authenticate before sensitive changes (email, password, MFA, payout details).

## Sessions

- Issue a new session id at login and at every privilege change; invalidate it on the server at logout.
- Cookies: `HttpOnly`, `Secure`, `SameSite`, a narrow `Path`, no `Domain` wider than needed; idle and absolute timeouts.

## Self-contained tokens (JWT and similar)

- Verify with an algorithm fixed in code, never the one the token's header names; reject `none`.
- Check `exp`, `nbf`, `iss` and `aud`; pick keys by `kid` only from your own key set.
- Nothing secret in the payload: it is only encoded. Plan revocation (short lifetime plus refresh, or a deny list).
- Prefer an `HttpOnly` cookie to `localStorage` for browser tokens, which any XSS can read.

## OAuth 2 and OpenID Connect

- Authorization code flow with PKCE, for confidential clients too; no implicit flow.
- Exact `redirect_uri` matching; `state` against CSRF and `nonce` against replay; validate the ID token's signature, issuer, audience and expiry.
- Link an external identity to a local account only by the provider's stable subject id, never by an unverified email.

## Links

- A01:2025 https://top10.owasp.org/2025/A01_2025-Broken_Access_Control/
- ASVS 5.0 chapters V6 to V10: https://github.com/OWASP/ASVS/tree/master/5.0/en
- OWASP Cheat Sheets (Authorization, Session Management, JSON Web Token, Server-Side Request Forgery Prevention, Cross-Site Request Forgery Prevention): https://cheatsheetseries.owasp.org/
