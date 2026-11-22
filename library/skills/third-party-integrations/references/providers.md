# Providers: headers, signatures, retries, test tools

Read when the task touches one of the providers or protocols below. Checked on each official page on 2026-11-22, restated in our own words.

Contents: HTTP retries · idempotency keys · Stripe · GitHub webhooks · Standard Webhooks · OIDC · SAML.

## HTTP retries

- `429 Too Many Requests` means rate limited, and the answer may carry `Retry-After` (RFC 6585 §4).
- `Retry-After` is either a number of seconds or an HTTP date (RFC 9110 §10.2.3); it also comes with `503` and with redirects.
- `GET`, `HEAD`, `OPTIONS`, `PUT` and `DELETE` are idempotent by definition (RFC 9110 §9.2.2); `POST` and `PATCH` are not and need an idempotency key to be retried safely.
- Full jitter: wait a random time between 0 and `min(ceiling, base × 2^attempt)`, never less than `Retry-After`, and stop at the attempt cap or the overall deadline.

## Idempotency keys

- The IETF header `Idempotency-Key` is still a draft (draft-ietf-httpapi-idempotency-key-header-09, 21 November 2026, not an RFC): https://datatracker.ietf.org/doc/draft-ietf-httpapi-idempotency-key-header/. In it the value is a structured-field string, in double quotes: `Idempotency-Key: "<uuid>"`. Stripe's header has the same name and takes the bare value. Follow each provider's own docs.

## Stripe

Docs: https://docs.stripe.com (official agent skills: `npx skills add https://docs.stripe.com`).

Keys (https://docs.stripe.com/keys)
- Sandbox: `pk_test_` (publishable), `rk_test_` (restricted), `sk_test_` (secret); live: `pk_live_`, `rk_live_`, `sk_live_`. Only publishable keys may reach a browser or an app. Stripe recommends restricted keys over secret keys.
- Sandbox and live objects never mix: an object made with test keys does not exist in live mode.

Idempotency, API v1 (https://docs.stripe.com/api/idempotent_requests, https://docs.stripe.com/error-low-level)
- Header `Idempotency-Key` on `POST` requests (no effect on `GET` and `DELETE`); up to 255 characters; a UUID v4 or another random value; no personal data in it.
- Stripe saves the status and body of the first request once the endpoint starts executing, errors and `500` included, and replays them for the same key; a replay carries `Idempotent-Replayed: true`. Reusing the key with other parameters is an error.
- Nothing is saved when the request fails validation, is rate limited or collides with a request still running with the same key (`409`): retry those with the same key and parameters.
- Keys are kept at least 24 hours, then may be dropped; a key reused after that starts a new request. Retry a request whose outcome is unknown within those 24 hours; past them, reconcile instead (fetch the object, check the dashboard, use webhooks).
- A `500` is indeterminate: the operation may have happened. Reconcile before any new key. Put a local id in the object's `metadata` so a webhook can be matched to the local record.
- `Stripe-Should-Retry: true` asks for a retry with backoff, `false` for none; absent, decide from the status.
- The SDKs can create keys and retry on their own (`max_network_retries` in Ruby, similar settings elsewhere; defaults differ by SDK).

Idempotency, API v2 (https://docs.stripe.com/api-v2-overview)
- `POST` and `DELETE` accept a key; a replay is the same key, same API, same account or sandbox, within 30 days. A retry finishes failed work rather than replaying the error. Without a key, Stripe makes one up.

Webhooks (https://docs.stripe.com/webhooks)
- Header `Stripe-Signature`: comma-separated entries, `t=<Unix seconds>` and one `v1=<hex>` per active secret; sandbox events add a `v0=` entry that is never valid; `secret`, `destination` and `payload` entries are unsigned debugging help. Accept unknown entries in any order and use only `t` and `v1`.
- Signed content: the `t` value, a `.`, then the raw request body. Signature: HMAC-SHA256 keyed with the endpoint's signing secret (`whsec_…`), compared in constant time with every `v1`; then reject when `t` is outside the tolerance. The official libraries default to 5 minutes; `0` turns the check off.
- Every delivery attempt has a new `t` and signature. Keep the server's clock synchronised (NTP).
- Secret rotation: the old secret can stay active up to 7 days, and each active secret gets its own `v1`.
- Answer `2xx` quickly, before slow work; a `3xx` is a failed delivery; HTTPS with TLS 1.2 or 1.3 in live mode.
- Automatic retries: up to three days with exponential backoff in live mode; three times over a few hours in a sandbox. Manual resend: 15 days from the dashboard, 30 days with `stripe events resend <event_id> --webhook-endpoint=<endpoint_id>`.
- No ordering guarantee; `created` is in seconds and shared by several events: never order or deduplicate by it. Deduplicate by event id; when one change yields two events, by `data.object` id plus `event.type`.
- A snapshot event's shape follows the API version of the account (or of the destination) when it was created. Thin events carry little: fetch the related object.
- Frameworks with CSRF protection (Rails, Django…) need the webhook route exempted.

Test environment (https://docs.stripe.com/testing)
- Card `4242 4242 4242 4242` with any future expiry and any CVC succeeds; `4000 0000 0000 0002` is declined (`generic_decline`); `4000 0000 0000 3220` needs 3D Secure. In server code and tests, prefer PaymentMethod ids such as `pm_card_visa` to card numbers. Real card details are forbidden in tests.

Stripe CLI (https://docs.stripe.com/cli/listen, https://docs.stripe.com/cli/trigger)
- `stripe login` is the user's step. Commands run in a sandbox by default; `--live` makes live requests: never use it.
- `stripe listen --events payment_intent.succeeded,charge.refunded --forward-to localhost:4242/webhook` forwards events to the local server and prints a `whsec_` signing secret for it, stable across restarts (`--print-secret` prints only that). The CLI reference requires `--events`, `--all-snapshot` or `--all-thin` with `--forward-to`; the webhooks guide still shows the shorter form, so pass the event list.
- `stripe trigger payment_intent.succeeded` makes real API calls in the sandbox: it creates the objects it needs, and those raise other events (`payment_intent.created` too). `--override`, `--add`, `--remove` change the fixture's parameters.

## GitHub webhooks

Docs: https://docs.github.com/en/webhooks/using-webhooks/validating-webhook-deliveries, https://docs.github.com/en/webhooks/using-webhooks/best-practices-for-using-webhooks

- `X-Hub-Signature-256: sha256=<hex>`: HMAC-SHA256 of the payload keyed with the webhook's secret. `X-Hub-Signature` (SHA-1) exists for old receivers only. Compare in constant time; handle the payload as UTF-8.
- No timestamp is signed: store `X-GitHub-Delivery` (unique per delivery, kept by a redelivery) to refuse replays and duplicates.
- `X-GitHub-Event` gives the event; the payload's `action` gives what happened.
- Answer `2xx` within 10 seconds, or GitHub ends the connection and counts a failure; process from a queue.
- GitHub never redelivers on its own: missed deliveries are redelivered by hand or through the API (https://docs.github.com/en/webhooks/using-webhooks/handling-failed-webhook-deliveries).
- GitHub's IP ranges come from `GET /meta` and change over time.

## Standard Webhooks

Spec 1.0.0, Apache-2.0: https://github.com/standard-webhooks/standard-webhooks/blob/main/spec/standard-webhooks.md (reference libraries for many languages: https://www.standardwebhooks.com/).

- Headers: `webhook-id` (the message id, unchanged across retries: use it as the idempotency key), `webhook-timestamp` (Unix seconds), `webhook-signature` (space-separated list such as `v1,<base64>`; several during a secret rotation).
- Signed content: `<webhook-id>.<webhook-timestamp>.<body>`. `v1`: HMAC-SHA256 keyed with the secret, which is `whsec_` followed by base64 of 24 to 64 bytes (the key is the decoded bytes). `v2a`: ed25519, keys `whpk_` / `whsk_` (`v1a` is deprecated).
- The spec asks for a timestamp tolerance without fixing it: use the reference library's check.
- For a sender: `2xx` is success, `3xx` a failure (update the URL instead of following it), `410` means disable the endpoint, `429`, `502` and `504` mean slow down; honour `retry-after`; retry over several days with exponential backoff and jitter; a 15 to 30 second request timeout; payloads kept small (under about 20 kB).

## OIDC

OpenID Connect Core 1.0 §3.1.3.7 (https://openid.net/specs/openid-connect-core-1_0.html) and the OAuth 2.0 Security BCP, RFC 9700 (January 2025, https://www.rfc-editor.org/rfc/rfc9700.html).

- Authorization code flow. PKCE with `S256`: required for public clients, recommended for confidential ones. No implicit flow, never the password grant.
- `state`: one-time, bound to the browser session, checked on the callback (CSRF). `nonce`: sent in the request, must come back unchanged in the ID token (replay).
- Redirect URIs match exactly at the provider. With more than one identity provider, check the `iss` returned with the authorization response (RFC 9207) against mix-up.
- ID token: signature checked with the issuer's published keys and the expected algorithm; `iss` equal to the issuer of the discovery document; `aud` containing the client id, and no untrusted extra audience; `azp` equal to the client id when present; current time before `exp`; `iat` not too far off; `nonce` equal to the one sent; `auth_time` checked when `max_age` was used.

## SAML 2.0

Web Browser SSO profile, §4.1.4.3 and §4.1.4.5 of https://docs.oasis-open.org/security/saml/v2.0/saml-profiles-2.0-os.pdf

- With the HTTP-POST binding the assertions must be signed: refuse an unsigned one. Check every signature against the identity provider's certificate from its metadata, never one carried in the message.
- Read the assertion the library returns as verified, not another element of the document: a response can hold a signed element and an unsigned one side by side.
- The bearer `SubjectConfirmationData`: `Recipient` equal to the assertion consumer service URL that received it; `NotOnOrAfter` not passed (a small clock skew at most); `InResponseTo` equal to the id of the request you sent, and absent only on an unsolicited response you chose to accept.
- `AudienceRestriction` contains your service provider's entity id; the `Issuer` is the expected identity provider.
- Replay: keep each assertion id until its `NotOnOrAfter` and refuse it a second time.
- Parse the XML with external entities and DTDs off.
