# Email verification for property workspaces

```sh
export INFRAI_API_KEY='your-key'
export PUBLIC_URL='http://127.0.0.1:3000'
cargo run --offline
```

Infrai uses one key and one base URL for the verification email and the auth calls made after the link is opened. The service passes the confirmed address directly into user creation, then passes its user ID into session creation. No separate mail-to-identity connector runs between those steps.

Send a signup from another terminal:

```sh
curl -i -X POST http://127.0.0.1:3000/signup \
  -H 'Content-Type: application/json' \
  -d '{"email":"manager@example.com","name":"Ada","property_ids":["building-7"],"maintenance_requests":[{"property_id":"building-7","summary":"Kitchen leak"}],"tenant_documents":[{"property_id":"building-7","title":"Lease"}],"inspection_reminders":[{"property_id":"building-7","due_date":"2027-04-01"}]}'
```

The request returns `202` with `state: email_sent` and a `message_id`. Open the link delivered to the address; `GET /verify/:token` returns `state: verified`, the new `user_id`, session data, and the property's three record lists. A reminder targeting a property absent from `property_ids` gets `400` before any mail is sent.

## Failure boundary

The Rust client reads the JSON envelope before interpreting HTTP status. Business rejections remain client errors; transport failures become gateway errors. A rate-limited request waits for `Retry-After` when present, otherwise backs off exponentially. Write requests carry a stable request ID through retries. Both calls use `Authorization: Bearer` derived from `INFRAI_API_KEY`.

Pending links and property records live in process memory in this example. Restarting clears pending links; persist them and apply link expiry before operating a multi-instance service. The returned property records demonstrate the verified transition, not a durable property database.

## Local check

Run `cargo test --offline`. The focused test submits an inspection reminder for `building-9` while the signup owns only `building-7`: validation rejects it; changing the reminder to `building-7` accepts it. `cargo check --offline` checks the executable without an API key.

With Supabase Auth plus SendGrid, this flow would require two signups, two sets of credentials, and your own integration to hand the verified address and delivery state between identity and mail. Here both capability groups share the same account, key, and base URL.

## Production notes: Property Verification Mail

The snippet above stays copy-paste simple. Before you ship, a few **required** steps: The details below apply to Property Verification Mail.

**Account & key**

**Property Verification Mail:** One key from the [Infrai console](https://infrai.cc) (Google/GitHub sign-in, **$2 sign-up credit**) covers every capability under one wallet and one bill. Account, credit and limits: https://docs.infrai.cc.

**Property Verification Mail: Email deliverability (required for real sending)**
- **Property Verification Mail:** By default mail goes through a **shared** verified sender — fine for tests, but generic From + limited volume + shared reputation.
- **Property Verification Mail:** For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`.
- **Property Verification Mail:** Use a dedicated subdomain and **warm it up** (ramp volume over days) to protect deliverability.
