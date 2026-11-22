---
name: third-party-integrations
description: >-
  Connects a product to services it does not own so that retries, duplicates, forged calls and provider outages
  corrupt nothing: outbound API clients with timeouts, retries limited to timeouts, 429 and 5xx with capped
  exponential backoff, jitter and Retry-After; idempotency keys; inbound webhooks verified on the raw body
  (Stripe-Signature, X-Hub-Signature-256, Standard Webhooks), deduplicated by event id and handled after a fast
  2xx; provider sandboxes, test cards and the Stripe CLI; OIDC and SAML single sign-on through a maintained
  library. Use when a task touches a payment provider such as Stripe, a webhook endpoint, a single sign-on
  integration, or a client for a third-party API. Not for the project's own API (back), the secrets store and
  infrastructure (devops), nor one-off HTTP calls from the shell (curl).
license: MIT
metadata:
  termf-stacks: "*"
  termf-roles: "integrations back"
  termf-versions: "*"
  termf-verified: "2026-11-22 Stripe docs (idempotency API v1 and v2, webhooks, CLI listen and trigger, keys, testing), GitHub webhooks docs, Standard Webhooks spec 1.0.0, draft-ietf-httpapi-idempotency-key-header-09, RFC 9110, RFC 6585, RFC 9700, OpenID Connect Core 1.0, SAML 2.0 profiles"
  termf-sources: "https://docs.stripe.com/api/idempotent_requests https://docs.stripe.com/api-v2-overview https://docs.stripe.com/error-low-level https://docs.stripe.com/webhooks https://docs.stripe.com/cli/listen https://docs.stripe.com/cli/trigger https://docs.stripe.com/keys https://docs.stripe.com/testing https://docs.stripe.com/skills https://docs.github.com/en/webhooks/using-webhooks/validating-webhook-deliveries https://docs.github.com/en/webhooks/using-webhooks/best-practices-for-using-webhooks https://docs.github.com/en/webhooks/using-webhooks/handling-failed-webhook-deliveries https://github.com/standard-webhooks/standard-webhooks/blob/main/spec/standard-webhooks.md https://datatracker.ietf.org/doc/draft-ietf-httpapi-idempotency-key-header/ https://www.rfc-editor.org/rfc/rfc9110.html https://www.rfc-editor.org/rfc/rfc6585.html https://www.rfc-editor.org/rfc/rfc9700.html https://openid.net/specs/openid-connect-core-1_0.html https://docs.oasis-open.org/security/saml/v2.0/saml-profiles-2.0-os.pdf"
---
# Third-party integrations

Calls out to, and webhooks in from, services the project does not own. The exact headers, tolerances and test tools of each provider are in `references/providers.md`. Stripe publishes its own agent skills (`npx skills add https://docs.stripe.com`): when one is loaded in this session, follow it for Stripe's products and API, and keep this skill for retries, idempotency, webhooks, keys and tests.

## Gotchas

- A timeout does not tell you whether the provider did the work. A create or a charge retried without an idempotency key, or with a new key on each attempt, can happen twice. Reuse one key, with the same parameters, for every attempt of one operation, and store it with the operation before the first attempt so a restart reuses it too.
- Stripe API v1 keeps a key for at least 24 hours and replays the first saved answer, a `500` included: a `500` leaves the outcome unknown. Reconcile it (fetch the object, wait for the webhook) before trying again with a new key. API v2 keeps keys 30 days and finishes failed work on a retry.
- A webhook signature covers the exact bytes received. A body parser that runs before the handler (JSON parsing, re-encoding, trimming) changes them and every check fails: read the raw body on that route only.
- Providers deliver at least once and in any order: Stripe retries for up to three days in live mode (three times over a few hours in a sandbox), and GitHub does not redeliver at all on its own. Deduplicate by event id; never order events by their timestamp.
- Stripe sandbox events carry an extra `v0` signature that is not valid, and `stripe listen` prints its own signing secret, different from the dashboard endpoint's. Check `v1` entries only, against the secret of the endpoint that sent the event.
- Two retry layers multiply: the Stripe SDKs retry on their own (`max_network_retries`). Keep one layer per call, the SDK's or yours.
- A tolerance of `0` in Stripe's libraries turns the timestamp check off instead of making it strict.

## Procedure

1. **Provider, SDK, API version.** Read the SDK version in the lockfile and the API version the project sends (Stripe pins one per account or per request), then open the provider's current docs for each endpoint and event the task uses. Use the provider's official SDK when the project has it. Done when you can name the SDK version, the API version and the doc pages, and the provider's docs agree with the ticket (when they disagree, report `blocked` with both).
2. **Keys.** TermF puts no key in an agent's environment. A key is usable only when the user already set it in the session (`STRIPE_SECRET_KEY`…): refer to it by its name in code and commands (`$STRIPE_SECRET_KEY`), never by its value, and never ask the user for one. Use test keys only (`sk_test_`, `rk_test_`); a live key (`sk_live_`, `rk_live_`) in the session is a reason to stop and report. The code reads keys and webhook secrets from the environment or the secret store the project already uses, never from a file in the repository. Done when the code reads every secret through the project's config and the `git grep` of `## Verify` prints nothing.
3. **Outbound calls.** Give every call a connect timeout and a total timeout. Retry only what may succeed later: connection errors, timeouts, `429` and `5xx` (and the provider's own retry signals, such as Stripe's `Stripe-Should-Retry` and `409` for a key still in use). Never retry another `4xx`. Back off exponentially with random jitter (a random delay between 0 and `min(ceiling, base × 2^attempt)`), a ceiling on each delay, a cap on attempts and an overall deadline; when the answer carries `Retry-After` (seconds or an HTTP date), wait at least that long. Done when every call site goes through one retry policy with its timeout, and a test shows a `400` is not retried.
4. **Idempotency.** Every call that creates, charges, refunds or sends something carries an idempotency key: an id stored with the local record (or a UUID v4 saved before the first attempt), reused with the same parameters for each retry. Changed parameters make a new operation and a new key, only once the first operation is known not to have happened. Keys hold no personal data. When a provider has no idempotency key, store your own operation id in the provider's metadata field and look it up before retrying. Done when a test that times out the first attempt sees the retry send the same key, and a second run of the operation creates nothing new.
5. **Inbound webhooks.** On the webhook route only: read the raw body, verify the signature with the provider's library (or by hand: HMAC of the signed content, constant-time comparison, timestamp inside the tolerance) before reading anything in it, and answer `400` without detail when it fails. Exempt that route alone from CSRF protection. Record the event id under a unique constraint and skip an id already recorded; answer `2xx` quickly (GitHub fails a delivery after 10 seconds) and do the work from a queue or a job. Do not depend on order: read the object's current state from the provider's API when it matters. Done when a duplicate delivery changes nothing, and a forged, stale or altered request is refused.
6. **Single sign-on.** Use a maintained OIDC or SAML library, never hand-written token or XML checks. OIDC: authorization code flow with PKCE (`S256`), a one-time `state` bound to the browser session, a `nonce` checked in the ID token, and an ID token validated for signature (issuer's published keys, expected algorithm), `iss`, `aud`, `azp` when present, and `exp`. SAML: signed assertions checked against the identity provider's certificate from its metadata, `Recipient`, `Audience`, `NotOnOrAfter`, `InResponseTo`, and each assertion id accepted once. Details in `references/providers.md`. Done when a test refuses each of: wrong issuer, wrong audience, expired token, missing or replayed nonce (OIDC); unsigned, expired, wrong audience or replayed assertion (SAML).
7. **Test environment.** Work in the provider's test mode or sandbox, never with a live account or real card. Stripe: test keys, test cards and `pm_card_visa`, `stripe listen` to forward events to the local server and `stripe trigger` to cause them (commands and their side effects in `references/providers.md`). The Stripe CLI works only once the user has run `stripe login`; when it has not, say in your report which commands the user can run. Done when every manual check you report ran against a sandbox.
8. **Tests of the failures.** Stub the provider at the HTTP boundary with the project's mocking tool, and sign webhook payloads inside the test with a secret the test makes up (never a real `whsec_`). Cover: the same event delivered twice; a bad signature; a stale timestamp; a body changed after signing; events out of order; a timeout then a retry with the same idempotency key; a `429` with `Retry-After`; a `5xx` retried until the cap; a `4xx` not retried. Done when each case is a test that fails without the code that handles it.
9. Run the commands of `## Verify` and put each one with its result in your summary, with, for each call and each webhook, its idempotency, retries, timeouts and the test environment used. Done when they pass, or when the failure is reported with its output.

## Verify

- The project's own commands first: its tests, typecheck and lint.
- `git grep -nE '(sk|rk)_(live|test)_[0-9A-Za-z]{10,}|whsec_[0-9A-Za-z]{10,}'` prints nothing.
- With the user's Stripe CLI logged in: `stripe listen --events <types> --forward-to localhost:<port>/<path>` in one terminal, `stripe trigger <event>` in another; the handler's log shows the event handled once.

## Security

- Nothing in a webhook is trusted before its signature checks out; after that, payload fields that grant access or money are confirmed against the provider's API.
- Logs carry event ids, types and outcomes, never raw bodies, signature headers, keys, tokens or card data.
- One signing secret per endpoint and per environment; while one rotates (Stripe keeps the old one up to 7 days), accept a signature from any active secret.
- A URL that comes from outside (a callback a customer registers, a link in a payload) is fetched only after its host is allowed, and never toward private or link-local addresses.
- Redirect URIs for single sign-on are registered exactly, and the identity provider's keys come from its metadata or discovery document, never from the message being checked.

## References

- Read `references/providers.md` when the task touches Stripe, GitHub webhooks, a provider that follows Standard Webhooks, OIDC or SAML, or HTTP retry headers: headers, signed content, tolerances, retry schedules, test cards and CLI commands.
