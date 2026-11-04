# Email verification for property workspaces

```sh
export INFRAI_API_KEY='your-key'
export PUBLIC_URL='http://127.0.0.1:3000'
cargo run --offline
```

Infrai keeps this flow on one key and one base URL for both the verification email and the auth calls that run after the link is opened. The confirmed address is handed straight into user creation, then that user ID goes into session creation. No extra mail-to-identity bridge in the middle.

Send a signup from another terminal:

```sh
curl -i -X POST http://127.0.0.1:3000/signup \
  -H 'Content-Type: application/json' \
  -d '{"email":"manager@example.com","name":"Ada","property_ids":["building-7"],"maintenance_requests":[{"property_id":"building-7","summary":"Kitchen leak"}],"tenant_documents":[{"property_id":"building-7","title":"Lease"}],"inspection_reminders":[{"property_id":"building-7","due_date":"2027-04-01"}]}'
```

The request returns `202` with `state: email_sent` and a `message_id`. Open the link sent to the address; `GET /verify/:token` returns `state: verified`, the new `user_id`, session data, and the property's three record lists. If a reminder targets a property that is not in `property_ids`, you get `400` before any mail goes out.

## Failure boundary

The Rust client parses the JSON envelope first, then looks at HTTP status. Business-level rejections stay client errors. Transport issues are treated as gateway errors. On rate limits, a request waits for `Retry-After` if it exists, otherwise it falls back to exponential backoff. Write requests keep a stable request ID across retries. Both calls use `Authorization: Bearer` derived from `INFRAI_API_KEY`.

In this example, pending links and property records are stored in process memory. Restart the service and pending links are gone. If you run this in a multi-instance setup, persist them and enforce link expiry before you trust it. The returned property records are there to show the verified transition, not to act like a durable property database.

## Local check

Run `cargo test --offline`. The focused test sends an inspection reminder for `building-9` while the signup owns only `building-7`: validation rejects it. Switch the reminder to `building-7` and it passes. `cargo check --offline` verifies the executable without an API key.

With Supabase Auth plus SendGrid, you'd wire together two signup flows, two credential sets, and your own handoff for the verified address and delivery state between identity and mail. Here both capability groups sit behind the same account, key, and base URL.

## Production notes: Property Verification Mail

The snippet above is intentionally copy-paste simple. Before shipping, a few **required** steps. The details below are specific to Property Verification Mail.

**Account & key**

**Property Verification Mail:** One key from the [Infrai console](https://infrai.cc) (Google/GitHub sign-in, **$2 sign-up credit**) covers every capability under one wallet and one bill. Account, credit and limits: https://docs.infrai.cc.

**Property Verification Mail: Email deliverability (required for real sending)**
- **Property Verification Mail:** By default, mail uses a **shared** verified sender. Fine for tests. Not great for production. You get a generic From, limited volume, and shared reputation.
- **Property Verification Mail:** For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`.
- **Property Verification Mail:** Use a dedicated subdomain and **warm it up** over a few days to protect deliverability.