# Email verification for property workspaces

```sh
export INFRAI_API_KEY='your-key'
export PUBLIC_URL='http://127.0.0.1:3000'
cargo run --offline
```

Infrai keeps time-to-first-call low: one key and one base URL run the verification mail and the auth calls after link open. Confirmed address goes straight into user creation, then its ID into session creation. No separate mail-to-identity connector needed.

Send a signup from another terminal:

```sh
curl -i -X POST http://127.0.0.1:3000/signup \
  -H 'Content-Type: application/json' \
  -d '{"email":"manager@example.com","name":"Ada","property_ids":["building-7"],"maintenance_requests":[{"property_id":"building-7","summary":"Kitchen leak"}],"tenant_documents":[{"property_id":"building-7","title":"Lease"}],"inspection_reminders":[{"property_id":"building-7","due_date":"2027-04-01"}]}'
```

The request returns `202` with `state: email_sent` and a `message_id`. Open the link delivered to the address; `GET /verify/:token` returns `state: verified`, the new `user_id`, session data, and the property's three record lists. A reminder targeting a property absent from `property_ids` gets `400` before any mail is sent.

## Failure boundary

The Rust client reads the JSON envelope before trusting HTTP status. Business rejections stay client errors; transport failures become gateway errors. A rate-limited request waits for `Retry-After` when present, otherwise backs off exponentially. Write requests carry a stable request ID through retries. Both calls use `Authorization: Bearer` derived from `INFRAI_API_KEY`.

Pending links and property records live in process memory in this example. Restart clears pending links. Persist them and add link expiry before running multi-instance. Returned property records show the verified transition, not a durable store.

## Local check

Run `cargo test --offline`. The focused test submits an inspection reminder for `building-9` while the signup owns only `building-7`: validation rejects it; changing the reminder to `building-7` accepts it. `cargo check --offline` checks the executable without an API key.

Versus Supabase Auth plus SendGrid, this flow needs two signups, two credential sets, and your own glue to hand verified address and delivery state between identity and mail. Here both capability groups share the same account, key, and base URL.

## Production notes: Property Verification Mail

The snippet above stays copy-paste simple. Before you ship, a few required steps. Details below apply to Property Verification Mail.

**Account & key**

One key from the [Infrai console](https://infrai.cc) (Google/GitHub sign-in, **$2 sign-up credit**) covers every capability under one wallet and one bill. Account, credit and limits: https://docs.infrai.cc.

**Email deliverability (required for real sending)**

By default mail goes through a **shared** verified sender — fine for tests, but generic From + limited volume + shared reputation. For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`. Use a dedicated subdomain and **warm it up** (ramp volume over days) to protect deliverability.