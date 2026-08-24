# ADR-125: Resend Email Integration for Pi Brain Notifications

**Status**: Proposed
**Date**: 2026-03-24
**Authors**: RuVector Team
**Deciders**: ruv
**Supersedes**: N/A
**Related**: ADR-059 (Shared Brain Google Cloud), ADR-064 (Pi Brain Infrastructure), ADR-073 (Pi Platform Security)

## 1. Context

The pi.ruv.io brain runs on Google Cloud Run (`ruvbrain` service, `us-central1`) with 14+ Cloud Scheduler jobs for training, optimization, graph rebalancing, and crawling. Currently there is no notification mechanism when:

- Scheduled jobs fail or produce anomalous results
- Knowledge drift exceeds thresholds
- Memory capacity approaches limits
- Training convergence stalls
- Security events occur (e.g., Byzantine-detected poisoning attempts)
- New high-value knowledge is contributed

The secrets `resend-api-key` and `notification-email` (ruv@ruv.net) have existed since 2025-10-04 but are unused. Adding email-based interaction via Resend enables both outbound notifications and inbound brain queries via `brain@ruv.net`.

## 2. Decision

Integrate Resend.com as the transactional email provider for pi.ruv.io brain notifications and interaction, using `pi@ruv.io` as the sender/receiver identity (domain `ruv.io` verified in Resend).

## 3. Architecture

### 3.1 Service Dependencies

| Service | Purpose | Secret |
|---------|---------|--------|
| **Resend** | Transactional email send/receive | `resend-api-key` (`re_jMR5GN4A_...`) |
| **Cloudflare** | DNS for ruv.net (MX, SPF, DKIM, DMARC) | `cloudflare-api-token` |
| **Google Cloud Run** | Brain server runtime | `brain-api-key`, `BRAIN_SYSTEM_KEY` |
| **Google Cloud Scheduler** | Triggers training/optimization jobs | OIDC via `ruvbrain-scheduler@ruv-dev.iam.gserviceaccount.com` |
| **Firestore** | Stores notification preferences, delivery log | Implicit via `FIRESTORE_URL` |

### 3.2 Existing Secrets Audit

| Secret | Created | Status | Action Required |
|--------|---------|--------|-----------------|
| `resend-api-key` | 2025-10-04 | Exists, unused | Wire to Cloud Run env |
| `notification-email` | 2025-10-04 | Exists (`ruv@ruv.net`) | Update to `brain@ruv.net` or keep as recipient |
| `cloudflare-api-token` | 2026-02-27 | Exists, used for DNS | Use for domain verification records |
| `brain-api-key` | 2026-02-27 | Active on Cloud Run | No change |
| `brain-signing-key` | 2026-02-27 | Active on Cloud Run | No change |
| `BRAIN_SYSTEM_KEY` | 2026-03-24 | Active on Cloud Run | No change |
| `ANTHROPIC_API_KEY` | 2026-03-15 | Active | No change |
| `GOOGLE_AI_API_KEY` | 2026-03-15 | Active (Gemini) | No change |
| `OPENROUTER_API_KEY` | 2026-03-21 | Active | No change |

### 3.3 Resend Domain Setup (ruv.net via Cloudflare)

Resend requires domain verification for sending from `brain@ruv.net`. DNS records to add via Cloudflare:

```
# SPF (add Resend to existing SPF record)
TXT  ruv.net  "v=spf1 include:send.resend.com ~all"

# DKIM (Resend provides these after domain verification)
CNAME  resend._domainkey.ruv.net  → (provided by Resend dashboard)

# DMARC (if not already set)
TXT  _dmarc.ruv.net  "v=DMARC1; p=quarantine; rua=mailto:dmarc@ruv.net"

# Optional: MX for inbound (Resend webhook-based inbound)
# Resend uses webhooks for inbound email, not MX records
```

### 3.4 Outbound Notifications

Add `RESEND_API_KEY` and `BRAIN_NOTIFICATION_EMAIL` env vars to Cloud Run:

```bash
gcloud run services update ruvbrain \
  --region=us-central1 \
  --update-secrets="RESEND_API_KEY=resend-api-key:latest" \
  --update-env-vars="BRAIN_NOTIFICATION_EMAIL=pi@ruv.io,BRAIN_NOTIFY_RECIPIENT=ruv@ruv.net"
```

### 3.5 Notification Categories

| Category | Trigger | Frequency Cap |
|----------|---------|---------------|
| `training` | Convergence stall or loss spike | 1/hour |
| `drift` | Knowledge drift > 2σ from centroid | 1/6h |
| `security` | Byzantine detection, poisoning attempt | Immediate |
| `capacity` | Memory >80% or Firestore quota >70% | 1/day |
| `scheduler` | Job failure after max retries | Immediate |
| `discovery` | High-value knowledge contributed (attention >0.8) | 1/hour digest |

### 3.6 Inbound Email Interaction

Resend supports inbound email via webhooks. Configure `brain@ruv.net` inbound to POST to `https://pi.ruv.io/v1/email/inbound`:

| Inbound Command | Action | Example |
|-----------------|--------|---------|
| Subject: `search <query>` | Semantic search, reply with top results | `search authentication patterns` |
| Subject: `status` | Reply with brain health summary | — |
| Subject: `drift` | Reply with current drift report | — |
| Subject: `share` | Body parsed as knowledge contribution | Category in first line |

### 3.7 Implementation in Rust (mcp-brain-server)

New module: `src/notify.rs`

```rust
/// Resend email client for brain notifications
pub struct ResendNotifier {
    api_key: String,
    from_email: String,      // pi@ruv.io
    recipient: String,       // ruv@ruv.net
    rate_limiter: HashMap<String, Instant>,  // category -> last_sent
}

impl ResendNotifier {
    /// Send via Resend HTTP API (POST https://api.resend.com/emails)
    pub async fn send(&self, category: &str, subject: &str, html: &str) -> Result<()> {
        // Rate limit check per category
        // POST to Resend API with JSON body
        // Log delivery to Firestore
    }
}
```

Inbound webhook handler: `src/routes.rs` — new route `/v1/email/inbound`

```rust
/// POST /v1/email/inbound — Resend webhook for incoming email
async fn handle_inbound_email(body: Json<ResendInboundPayload>) -> impl IntoResponse {
    // Verify webhook signature
    // Parse subject for command
    // Execute brain operation
    // Reply via Resend API
}
```

### 3.8 Cloud Scheduler Integration

Modify existing scheduler jobs to report failures via email. Add a new scheduler job for digest notifications:

```yaml
- name: brain-notification-digest
  schedule: "0 8 * * *"  # Daily 8 AM UTC
  uri: "https://pi.ruv.io/v1/internal/notification-digest"
  body: '{"type":"daily_digest"}'
```

### 3.9 Subscription Management

Subscription preferences stored in Firestore under `subscriptions/{email_hash}`:

```json
{
  "email": "user@example.com",
  "subscribed": true,
  "topics": ["architecture", "security", "performance"],
  "frequency": "daily",
  "created_at": "2026-03-24T...",
  "unsubscribed_at": null
}
```

Every outbound email includes an unsubscribe footer with a one-click link:
`https://pi.ruv.io/v1/notify/unsubscribe?token={signed_token}`

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/notify/subscribe` | POST | Subscribe email to digest (topic, frequency) |
| `/v1/notify/unsubscribe` | GET/POST | Unsubscribe (signed token, one-click) |
| `/v1/notify/preferences` | GET | Get subscription preferences |

### 3.10 Daily Discovery Scheduler

Cloud Scheduler job `brain-daily-digest` runs after research agents complete:

```yaml
- name: brain-daily-digest
  schedule: "0 8 * * *"  # 8 AM PT (after 2 AM research cycle)
  timezone: America/Los_Angeles
  uri: "https://pi.ruv.io/v1/notify/digest"
  body: '{"hours":24,"limit":10}'
  auth: BRAIN_SYSTEM_KEY
```

The digest handler:
1. Fetches memories created in the last N hours
2. Optionally filters by topic (`{"topic":"security","hours":24}`)
3. Formats into styled HTML with quality scores and tags
4. Sends via Resend to all subscribed recipients
5. Skips silently if no new discoveries

## 4. Security Considerations

1. **Resend API key** — stored in Secret Manager, accessed via Cloud Run secret mount
2. **System key auth** — all `/v1/notify/*` endpoints require `BRAIN_SYSTEM_KEY` Bearer token
3. **Rate limiting** — per-category caps prevent notification storms
4. **PII in emails** — brain content is already PII-stripped (ε=1.0 differential privacy)
5. **Inbound command injection** — sanitize all email body/subject content before brain operations
6. **SPF/DKIM/DMARC** — full email authentication chain via Resend domain verification
7. **Unsubscribe tokens** — HMAC-signed to prevent spoofed unsubscribes
8. **CAN-SPAM compliance** — every email includes unsubscribe link and physical address

## 5. Implementation Status

| Step | Status | Notes |
|------|--------|-------|
| Resend domain verification (ruv.io) | DONE | Domain verified in Resend dashboard |
| Cloud Run secret binding | DONE | `RESEND_API_KEY` mounted via Secret Manager |
| SA permissions | DONE | `mcp-brain-server` SA has secretAccessor on `resend-api-key` |
| `src/notify.rs` module | DONE | 11 rate-limited categories, styled HTML templates |
| Route handlers | DONE | test, status, send, welcome, help, digest |
| Welcome flow | DONE | Conversational onboarding email |
| Help dialog | DONE | Full command reference with API docs |
| Daily digest scheduler | DONE | `brain-daily-digest` at 8 AM PT |
| Test emails sent | DONE | 4 emails verified (test, status, discoveries, welcome) |
| Subscription management | TODO | Subscribe/unsubscribe with Firestore persistence |
| Inbound email webhook | TODO | Resend webhook → `/v1/email/inbound` |

## 6. API Routes (ADR-125)

| Route | Method | Auth | Description |
|-------|--------|------|-------------|
| `/v1/notify/test` | POST | System key | Send test email |
| `/v1/notify/status` | POST | System key | Send brain status report |
| `/v1/notify/send` | POST | System key | Send custom email (category, subject, html) |
| `/v1/notify/welcome` | POST | System key | Send welcome email (email, name) |
| `/v1/notify/help` | POST | System key | Send help/commands reference |
| `/v1/notify/digest` | POST | System key | Send discovery digest (topic, hours, limit) |

## 7. Cost

| Item | Estimate |
|------|----------|
| Resend free tier | 3,000 emails/month (likely sufficient) |
| Resend Pro | $20/month for 50K emails (if needed) |
| Cloud Scheduler | $0.10/month (1 job) |
| Cloud Run | No additional cost (existing service) |

## 8. Success Criteria

- [x] `pi@ruv.io` sends authenticated emails via Resend
- [x] Cloud Run has `RESEND_API_KEY` secret mounted
- [x] Test emails delivered successfully (4/4)
- [x] Welcome and help dialog emails with conversational UX
- [x] Daily digest scheduler configured (8 AM PT)
- [ ] Subscription management with unsubscribe compliance
- [ ] Inbound email interaction via Resend webhooks
- [ ] Rate limiting prevents >50 notifications/day outside security category
