# Jumpstart parity — gap analysis

**Date:** 2026-10-01 · **Branch:** `audit/jumpstart-parity` · **Author:** audit pass, not a packet
**Scope:** the five areas named by the product owner — identity/auth, courier, observability,
errors, frontend.
**Method:** every "we have it" claim below was read out of source in
`/Users/kaka/Code/any/moon/cafaye/` and carries a `file:line`. Every "we do not" claim
names the search that proved it. No CHANGELOG entry was accepted as evidence of a
feature; several CHANGELOG claims were checked against source and found false (see §1.4).

---

## 0. Summary

The fleet's **back half is genuinely strong and its front half does not exist**. On paper
identity is ahead of any starter kit — argon2id, DB-backed opaque sessions with SHA-256
digests, TOTP with a set-based replay guard and 10 recovery codes, a real OIDC *provider*,
scoped owner-only API keys, and an audit log that refuses `UPDATE`/`DELETE` at the database
level (`identity/migrations/00012_account_audit_log.sql:131-141`). courier has a real SMTP
path, real templates, a real Oban queue config and the only working error tracker in the
fleet. `kit` ships a complete LGTM stack. What none of that adds up to is a product: **there
is not one email address that this platform can send**, because identity's mailer is the
refusing `Unavailable{}` (`identity/cmd/identity/main.go:498`), courier's send path is a
library call with no HTTP route into it (zero matches for `Deliver|Mailer` in
`courier/lib/courier_web/`), and no service calls courier (`identity/cafaye.yml:188` says
"identity is a leaf"). Consequently password reset, email verification, email change and
team invitations are all reachable-looking endpoints that cannot complete — stated plainly
at `identity/DECISIONS.md:664-667`: *"a user who forgets their password cannot recover."*
Layered on top: the only customer-facing app, `parlor`, is an eight-page Next.js template
whose login **does not work against a real stack** (`parlor/README.md:313-319`), and
identity's README advertises OAuth social login that has a complete library, a database
table and **no route and no wiring** (`identity/README.md:1266`). **The single biggest gap is
that nothing can send an email** — most of the other gaps in this report sit behind it, and
it is also the cheapest one to fix relative to what it unlocks.

**A correction to the brief's premise, stated plainly:** cafaye does *not* have "no
customer-facing UI". It has three HTML surfaces: `parlor` (a Next.js 16 app shell, eight
pages, its own component library), `docs` (a live Astro/Starlight documentation site at
docs.cafaye.com with `index.mdx` and `pricing.md`), and one hand-written unstyled Go
sign-in page inside identity (`identity/internal/httpapi/oidc.go:854-884`). What it does
**not** have is any of them being a finished product. `parlor` says so itself at
`parlor/README.md:12-13` ("This is a template, not a platform-as-a-service dependency") and
`parlor/AGENTS.md:14-16` ("If you are looking for a dashboard, settings, admin, shadcn/ui
or a subscription screen, you are looking at a later packet"). There is **no LiveView
anywhere in the fleet** — zero `.heex`/`.leex` files, zero `mount/3`, zero
`Phoenix.LiveView` in any dependency.

---

## 1. What Jumpstart actually ships — and why §1 is not what was asked for

### 1.1 The named parity bar does not exist at the named address

`jumpstartpro.com` **is a parked domain offered for sale.** This is not a transient outage;
it is the registered state of the domain.

| Evidence | Result |
|---|---|
| `dig NS jumpstartpro.com` | `ns1.afternic.com`, `ns2.afternic.com` (Afternic = domain-parking registrar) |
| `dig A jumpstartpro.com` | `76.223.54.146`, `13.248.169.48` (Afternic parking anycast) |
| `dig A docs.jumpstartpro.com` | the same two IPs |
| `GET https://jumpstartpro.com/` | 114 bytes: `<script>window.onload=function(){window.location.href="/lander"}</script>` |
| `GET https://jumpstartpro.com/lander` | **403** — *"You don't have permission to access `http://forsale.godaddy.com/forsale/jumpstartpro.com`"* |
| `GET https://docs.jumpstartpro.com/` | the identical 114-byte redirect (same parking infrastructure) |
| Wayback CDX for `docs.jumpstartpro.com*` | **zero snapshots, ever** |
| Wayback CDX for `jumpstartpro.com` | one snapshot, `20020716130405` (before the product existed) |
| `api.github.com/users/jumpstartpro` | 404 — no such account |
| `api.github.com/users/jumpstartpro/repos` | 404 |
| hex.pm search `jumpstart`, `jumpstartpro` | no packages |
| npm search `jumpstartpro` | no packages |
| boilerplate catalogues (`boilerplatelist.com`, `starterindex.com`) | list "Jumpstart" **only**, tagged *Ruby on Rails* |

**Consequence:** the product owner cannot verify a feature against `jumpstartpro.com`,
because it is a for-sale page. Any gap in this report measured against "the Phoenix Jumpstart
Pro" is therefore measured against a product **I could not read.**

### 1.2 There are two different products called "Jumpstart Pro"

| | Jumpstart Pro **Rails** | Jumpstart Pro (Phoenix) |
|---|---|---|
| Address | `jumpstartrails.com` — **live** | `jumpstartpro.com` — **parked, for sale** |
| Stack | Rails 8, Hotwire, Tailwind, importmap/Stimulus | Phoenix / LiveView (per the brief; unverifiable) |
| GitHub | `github.com/jumpstart-pro` exists, 1 public repo (`example-hotwire-native-rails-backend`) | none |
| Docs | 50 pages, fully crawlable | unreachable |
| Relationship | `jumpstartpro.relationkit.io` (its help centre) links to `jumpstartrails.com` | — |

I found **no statement** from either product connecting them as one lineage. They may be the
same team at different times, or unrelated products sharing a name. I did not assume either.

### 1.3 What this report measures against, and the honest caveat

**Every Jumpstart citation below is `jumpstartrails.com/docs/...` — the Rails product.** It
is a live, first-party, citable documentation set, it covers the same five areas, and it is
the closest verifiable analogue to the named bar. I used it as a **proxy** and labelled it.

**The caveat that must travel with every table in §2–§6:** a Phoenix/LiveView Jumpstart Pro
would plausibly have differed from the Rails product in at least three ways that would change
gap *sizes* — it would have shipped LiveView components rather than Hotwire partials, its
auth story would have been built on `pow`/`phx.gen.auth` rather than Rails 8's auth
generator, and its email would have been Swoosh/letter_opener rather than ActionMailer. I
have no evidence either way. **The *direction* of every finding below is safe** (cafaye has no
UI product, no email delivery, no OAuth routes, 1-of-13 error tracking). **The *sizes* are
provisional.**

### 1.4 Findings about Jumpstart that are as important as its features

Four things the brief assumed are **not** in the docs, and I am recording them rather than
silently filling them in:

1. **No two-factor / passkey support is documented.** I grepped all 50 documentation pages for
   `two-factor`, `2fa`, `totp` — **zero hits**. Not in the nav, not in the authentication page.
   The auth page documents registration, sessions, passwords, email change, and OAuth
   (`https://jumpstartrails.com/docs/authentication`). **UNVERIFIED, leaning absent.**
2. **No magic link / passwordless is documented.** Grepped `magic link`, `passwordless` —
   **zero hits** across 50 pages.
3. **No OpenTelemetry, no tracing, no metrics, no dashboards.** Grepped `opentelemetry`,
   `open telemetry`, `prometheus`, `grafana`, `loki`, `observability`, `monitoring`, `health
   check` — the only hit for `monitoring` is the third-party-integration list on
   `https://jumpstartrails.com/docs/integrations`. **Observability is not a Jumpstart feature
   as far as its own docs are concerned.** This is the area where cafaye is *ahead*.
4. **No local mail preview is documented.** Grepped `letter_opener`, `swoosh`, `premailer`,
   `blazer` — **zero hits**. The email page lists ten providers and credentials, and nothing
   about reading a sent message in development.

### 1.5 Jumpstart Pro Rails — verified feature inventory, by area

All URLs are `https://jumpstartrails.com/docs/<page>`, fetched 2026-10-01.

**Identity / auth**
- Rails 8's built-in authentication generator. Sessions **database-backed**, signed http-only
  cookie, `has_secure_password`. *"no third-party authentication dependencies"*.
  bcrypt digests from older Devise installs keep verifying (`/authentication`).
- **OAuth / social login via OmniAuth**, enabled per-provider from a config wizard at
  `/jumpstart`. The docs enumerate the four social-auth situations handled: new user with an
  OAuth account; connecting an OAuth account to a logged-in user; signing in with a previously
  connected account; **rejecting** login to an existing user who has not connected that
  social account. Button pattern is `button_to … omniauth_authorize_path(:twitter)` with
  `data: { turbo: false, disable_with: … }` — the POST is deliberate, for security
  (`/authentication`). Providers are configured, not enumerated in docs; `twitter` appears in
  the example, which is not a claim about support.
- **Email change** (`Users::EmailChangesController`): writes `unconfirmed_email`, requires
  the address not belong to another account, single-use link **expiring in one hour**,
  notifies the previous address so the owner learns of an unauthorised change, asks for no
  password (so it works for OAuth sign-ups), and is **rate limited** (`/authentication`).
- **Accounts = teams + personal accounts.** *"The name Account is a generic term but can be
  thought of as a vehicle for creating teams or organizations as well as personal accounts."*
  A personal (solo) account is created on sign-up and cannot invite; further accounts are team
  accounts. Personal-account creation is a wizard toggle (`/accounts`).
- **Roles.** Users carry a global `staff` flag for `/admin`. Per-account roles are *whatever
  you declare* — *"By default, we only provide an admin role, but you can add more roles in
  `app/models/account_user.rb`"* — surfaced in the member-add UI. Helpers `Current.account_user`
  and `Current.roles #=> [:admin]` (`/roles`).
- **Multi-tenancy.** `current_account` scoping, selectable in the wizard, with optional
  `acts_as_tenant` per model as a safety net for forgotten scoping (`/multitenancy`).
- **API tokens**: per-user, multiple, revocable, with `last used at` tracking, plus a
  `POST /api/v1/auth.json` email+password → token exchange for mobile
  (`/api`).

**Courier / transactional email**
- **Ten providers**, each pre-configured: Amazon SES, Mailgun, Mailjet, Mandrill (Mailchimp),
  MailPace, Postmark, Resend, Sendgrid, Brevo (Sendinblue), Bird (Sparkpost). Default from
  address is configurable (`/email`).
- **Background workers are a wizard choice, not a default**: your own ActiveJob adapter,
  Async (explicitly *"Should not be used in production"*), Solid Queue (SQL), or Sidekiq
  (needs Redis). Selecting one adds the gem and wires it for all environments. If Sidekiq is
  on, the **Sidekiq Web UI is enabled and restricted to admins** and linked in navigation
  (`/background_workers`).
- **Scheduled jobs**: `whenever` with `config/schedule.rb`, or Sidekiq-Cron; guidance is that
  cron should *kick off* workers rather than do work (`/cron`).
- **Notifications** are the `noticed` gem, multi-medium: **Database, Email, ActionCable,
  Slack, Twilio, Vonage**. `ApplicationNotifier` parent class, `deliver_later`, a navbar
  notifications menu via ActionCable, browser Notification API, and tenant scoping through a
  `belongs_to :account` (`/notifications`).

**Observability / monitoring**
- **Third-party error & performance monitoring only**, configured by wizard
  (`/integrations`): **AppSignal** (recommended, error + performance), **Honeybadger**
  (recommended, error + uptime), AirBrake, Rollbar, Scout APM, Sentry, Skylight. Bugsnag
  appears in the credentials guide (`/credentials`). **No first-party tracing, metrics or
  dashboards are documented.**
- Deploy recipes: staging + production environments preconfigured; **email providers
  configured for production only** so staging cannot mail real users; four Solid databases
  (primary, queue, cache, cable); Hatchbox.io, Render (`render.yaml`), Heroku (`Procfile`,
  `app.json` deploy button); Stripe CLI added to `Procfile.dev` for local webhook forwarding
  (`/deploying`).

**Errors**
- Covered above: the seven-to-eight configurable error trackers on `/integrations`, plus a
  Sidekiq Web UI restricted to admins (`/background_workers`). **No JS/browser error capture
  and no LiveView `ErrorBoundary` behaviour is documented.**

**Frontend**
- **Tailwind CSS** as the design system, purge-configured, customisable via
  `app/assets/stylesheets/application.tailwind.css`; Tailwind Plus recommended as a paid
  complement (`/tailwind`).
- **Documented component set** (one docs page each): Alerts & Notices, Buttons, Cards,
  Clipboard, Forms, Modals, Navigation, Pagination, Pills, Tabs, Tooltips, Slideover, Wells
  — plus Branding, Icons (Heroicons, recommended), JavaScript (Stimulus + esbuild),
  Themes (light + dark defaults), Typography (`/docs` nav; each page above).
- **Forms plugin** included for default field styling (`/forms`).
- **Admin area at `/admin`**: madmin-generated per-model dashboards, **a metrics dashboard
  (total and recent revenue, user count, active subscriptions, customisable)**, user
  impersonation with a persistent banner and logout, and **`/admin` returns 404 for
  non-admins** (`/admin`).
- **Announcements** feature with a navbar "What's New" entry and unread red dot
  (`/announcements`).
- Marketing site: not documented as a component of the product.

---

## 2. Identity / auth

Legend for "we have it?": **YES** verified in source · **PARTIAL** · **NO** (search named) ·
**N/A** deliberate divergence.

| Jumpstart feature | We have it? | Where | Gap | Notes |
|---|---|---|---|---|
| Password registration + login | **YES** | `identity/internal/users/password.go:7,37-40`; login route `internal/httpapi/auth.go:130`; `users.password_digest` `migrations/00002_users.sql:28` | — | argon2id. Comparable or better than Jumpstart's `has_secure_password` (bcrypt). |
| DB-backed session, signed http-only cookie | **YES (stronger)** | `identity/migrations/00003_sessions.sql:21-30` (SHA-256 digest, opaque); `internal/httpapi/auth.go:30` (`__Host-session` cookie, Secure/HttpOnly/SameSite=Lax) | — | Jumpstart uses a signed cookie; cafaye stores a digest, so a DB read cannot recover the token. |
| OAuth / social login (OmniAuth, multi-provider) | **NO — library only** | `internal/oauth/` is complete (`provider.go:81-106` Google+GitHub, `client.go`, `store.go`, `cipher.go`); `migrations/00008_connected_accounts.sql:42-81` exists. **But** `internal/httpapi/oauth.go` does not exist, no `/v1/auth/oauth` route is mounted, `cmd/identity/main.go:24-37` does not import `internal/oauth`, and every constructor call is in `_test.go` | **M** | **The largest single identity gap.** `identity/README.md:1266` claims `- [x] OAuth (social login) via goth`; `goth` is not in `go.mod` (only `golang.org/x/oauth2 v0.37.0`, `identity/go.mod:18`). The table is dead schema. |
| OAuth sign-up linking rules (new / connect / sign-in / reject) | **NO** | — | included in OAuth packet | Would be implemented by the same route work. |
| Email change, unconfirmed-address, 1h single-use link, notify old address, rate-limited | **PARTIAL — flow exists, delivery does not** | `POST /v1/email-changes` etc. `internal/httpapi/recovery.go:88`; `migrations/00015_recovery_tokens.sql:28-107` (purpose `email_change`) | **S/M** | Completes only if a mailer exists. `identity/cmd/identity/main.go:498` wires `recovery.Unavailable{}`, whose `Send` returns `ErrNoMailer` (`recovery.go:290-296`), so all six send-routes answer 503. |
| Email verified on sign-up | **NO (deliberate)** | `migrations/00014_recovery_tokens.sql:29-30` adds `email_verified_at`; sign-up does not require it | **S** | Aligns with Jumpstart (*"Addresses are not confirmed on sign up"*) — **not a gap**. |
| Two-factor / TOTP | **YES (stronger)** | `internal/mfa/` (12 files); `pquerna/otp` `go.mod:12`; 5 routes `internal/httpapi/mfa.go:130-134`; tables `migrations/00010_mfa.sql` including a **set-based replay guard** `mfa_used_totp_steps` PK `(credential_id, step)` at `:220-238` and 10 recovery codes `:149-177` | — | Jumpstart documents no 2FA at all (§1.4.1). **cafaye is ahead.** |
| WebAuthn / passkeys | **NO** | grep `webauthn\|passkey\|FIDO` in `internal/**` → 2 hits, both comments (`internal/mfa/mfa.go:56`); `migrations/00010_mfa.sql:89-90` is `CHECK (method = 'totp')` | **M** | Not a Jumpstart feature either. Optional. |
| API tokens, multiple per user, revocable, last-used tracking | **YES (stronger)** | `internal/apikeys/`; `migrations/00011_api_keys.sql:21-158` (scopes, `expires_at`, `last_used_at`, `revoked_by`, `revoke_reason`); `internal/httpapi/apikeys.go:76-78`; prefix `cafaye_` `internal/apikeys/apikeys.go:75` | — | 6 named scopes, owner-only, DB-revoked. |
| Email+password → token exchange for mobile | **NO** | — | **S** | cafaye's `POST /v1/session` returns `{token, expires_at}` and serves the same need (`internal/httpapi/auth.go:129`). Arguably already covered. |
| Accounts = teams + personal accounts | **YES** | `migrations/00005_accounts.sql:18-21,31` (personal account on sign-up); membership `migrations/00006_account_users.sql:29-51`; `GET /v1/accounts` returns every account (`internal/httpapi/accounts.go:568`) | — | Same model, same vocabulary ("account"), same personal-vs-team distinction as Jumpstart's `/accounts`. |
| Per-account roles, customisable | **PARTIAL** | 3 roles `owner`/`admin`/`member`, ordered enum `internal/accounts/accounts.go:157-205`; DB enum `migrations/00005_accounts.sql:25` | **S** | Jumpstart ships `admin` + "add more in account_user.rb". **cafaye's own source names Jumpstart as the thing it rejected** — see §8.2. → divergence, not a gap. |
| `current_account` scoping + `acts_as_tenant` safety net | **NO — by decision** | no current-account concept in any of 16 tables; `accounts` claim is deliberately an array (`identity/README.md:839-843`); scoping is per-request from the path | **M** | → **divergence §8.1**. |
| Platform admin (`staff` flag, `/admin` area, impersonation) | **PARTIAL** | 3 JSON admin routes, scoped-key-only (`internal/httpapi/admin.go:107-129`, `adminPrefix` `:73`); audit log read. **No staff flag, no impersonation, no HTML admin** | **M** | Recorded as deliberately undecided (`identity/DECISIONS.md:202-208`). |
| Impersonation | **NO** | — | **S/M** | Requires a deliberate decision. |
| Rate-limited email change | **NO** | grep `rateLimit\|rate_limit` in `identity/internal/**` → 0 | **S** | Jumpstart documents rate limiting here; cafaye does have account lockout on login (`internal/sessions/lockout.go`, 5 attempts → 423). |
| LiveView `on_mount` hooks / `current_user` in LiveView | **N/A** | no LiveView in the fleet | — | → **divergence §8.3**. |
| Session cookie **or** BFF | **PARTIAL** | cookie is correct (`__Host-`); but `parlor` stores the token in `localStorage` because the browser calls identity cross-origin (`parlor/README.md:151-172,313-319`) | **S-M** | The API is right; the client chose the weak option to work around its own bug. → packet p-04. |
| Any signup / login / reset **form** | **NO** | glob `**/*.{js,ts,css,scss,heex,eex,html}` in `identity/` → no files; only markup is a 31-line Go template `internal/httpapi/oidc.go:854-884` | **L** | → packet p-07/p-08 territory, in `parlor`. |

**Also found, not asked about — 12 undocumented operations.** identity's OpenAPI drift test
pins 12 routes that exist in code but not in the contract (`internal/httpapi/openapi_drift_test.go:126-144`):
10 tenancy routes plus `POST /oidc/authorize` and `POST /oidc/userinfo`. Seven of the ten are
scope-gated (`internal/httpapi/accounts.go:150-190`) — **authorized in code, absent from the
document a client is generated from.**

---

## 3. Courier / transactional email

| Jumpstart feature | We have it? | Where | Gap | Notes |
|---|---|---|---|---|
| Multiple providers natively supported | **PARTIAL — one adapter** | `courier/lib/courier/mailer.ex:32` is a one-line Swoosh mailer; `lib/courier/mailer_adapter.ex:136-137` (`smtp`), `:133-134` (`none`→Local); `config/config.exs:50` sets `api_client: false` | **M** | Jumpstart ships **10** configured providers (`/email`). cafaye ships SMTP only; SES/Postmark/Mailgun/SendGrid/Resend reachable **only through their SMTP front** (`mailer_adapter.ex:146-147`). Two boot gates refuse to start non-delivering (`config/runtime.exs:123`, `application.ex:32`). |
| An HTTP endpoint that sends a message | **NO — none exists** | grep `Deliver\|Mailer` across `courier/lib/courier_web/` → **zero matches**; the send API is `Courier.Deliver.send_and_record/3` (`lib/courier/deliver.ex:100-120`), called only from itself and tests; `cafaye.yml:84` `consumes: []` | **M** | **This is the root of the biggest gap in the report.** courier is unreachable by anything over HTTP. |
| Background delivery queue | **PARTIAL — queue configured, never used** | `config/config.exs:54-56` (`queues: [outbox: 10, webhooks: 5]`); 3 workers (`lib/courier/workers/*.ex`); `Oban.insert` → **zero matches**; no cron plugin | **M** | Email send is **synchronous and holds a DB transaction open across the provider call** — stated as deliberate at `lib/courier/deliver.ex:95-99`. |
| Retry / backoff | **YES** | outbox exponential+jitter, cap 60s `lib/courier/workers/process_outbox_worker.ex:42,82-91`; webhooks base 300s, cap 6h, 8 attempts `config/config.exs:68-96` | — | Genuinely good. |
| Local mail preview / read the sent message | **NO** | `config/dev.exs:81-82` — *"The Swoosh preview plug is deliberately not wired — courier has no HTML layer to preview into."* `Swoosh.Adapters.Local` renders to memory and **nothing reads it** | **S** | `letter_opener`/`Swoosh.Local` equivalent absent. Jumpstart documents none either (§1.4.4) — so this is parity-by-both-being-absent, but it blocks courier's own dev loop. |
| Email templates organised per type, HTML + text | **YES** | 8 compiled EEx files `lib/courier/mailers/templates/` (`layout`, `welcome`, `password_reset`, `team_invitation` × `.html`/`.text`); build-time compile `lib/courier/mailers/templates.ex:27-29`; `@external_resource` `:18-25` | — | Plain EEx, not HEEx — deliberate (`lib/courier/mailers.ex:48-55`). |
| More than 3 email types | **NO** | `@types ~w(welcome password_reset team_invitation)` `lib/courier/mailers.ex:68` | **M** | Jumpstart has no fixed list either (it is Rails mailers you write), so this is a *cafaye* limit, not a parity gap. |
| Bounce / complaint handling | **NO** | `cafaye.yml:63-65` — *"email.bounced and email.complained need a provider webhook, which is a later packet"*; 4 of 5 catalogued events never emitted (`lib/courier/events.ex:24-25`) | **M** | Not a Jumpstart feature per its docs. |
| Notifications beyond email | **PARTIAL** | notification preferences + API `internal/courier_web/router.ex:59-60`; **outbound webhooks fully built** (signature, deliveries table, circuit breaker); **push is a column that always evaluates false** (`lib/courier/notification_preferences.ex:133`); no SMS, no in-app | **L** | Jumpstart's `noticed` gives Database/Email/ActionCable/Slack/Twilio/Vonage (`/notifications`). cafaye's equivalent is an outbound webhook service — a **deliberate divergence, §8.4**. |
| Sidekiq Web UI restricted to admins | **N/A** | `lib/courier_web/endpoint.ex:23` serves only `priv/static/{robots.txt,favicon.ico}`; `AGENTS.md:10-12` — *"API only: no HTML, no LiveView, no assets, no dashboard"* | — | → **divergence §8.5**. |
| Scheduled jobs (`whenever` / Sidekiq-Cron) | **NO** | no cron plugin in `config/config.exs:54-56`; no NATS (`lib/courier/nats_publisher.ex:33` defaults to `Noop`, no `gnat` dep) | **M** | kit's `templates/` carry no cron recipe. |
| Committed OpenAPI document | **YES** | `courier/openapi.yaml`, 3.1.0, `info.version: 2.0.0` `:164`, `security: bearerAuth` `:198-199`, bidirectional router check `test/courier_web/openapi_document_test.exs` | — | Stronger than Jumpstart (which documents no machine contract for itself). |

---

## 4. Observability / monitoring

**Read this row first:** Jumpstart documents **no first-party observability at all** (§1.4.3).
cafaye has substantially more, spread across `core` schemas and `kit` templates. The gaps in
this section are therefore *cafaye's own* stated gaps, not parity deficits — except the
"adoption" gaps, which are real and severe.

| Jumpstart / expected feature | We have it? | Where | Gap | Notes |
|---|---|---|---|---|
| Error & performance monitoring integrations | **PARTIAL — 1 service** | `courier/mix.exs:150` (`sentry ~> 13.5`), off by default (`lib/courier/error_reporting.ex:102-107`), own relay `lib/courier/error_relay.ex`, policy `error_relay/policy.ex:120-134`, self-hosted GlitchTip `docker-compose.errors.yml:137` | **L** | grep `sentry\|glitchtip\|bugsnag\|honeybadger\|rollbar` across every manifest in the fleet → the only non-test hits are in `courier`. See §5. |
| OpenTelemetry at request granularity | **PARTIAL — 3 of 13** | `identity/go.mod:14-17` + spans `internal/httpapi/telemetry.go:72`; `billing/Gemfile:72-73` + `lib/middleware/request_telemetry.rb:72`; `courier/mix.exs:104-105` + `lib/courier_web/plugs/telemetry.ex:89` | **L** | `pantry` (`Cargo.toml:52-53`), `darkroom` (`Cargo.toml:46-47`), `guard` (`package.json:18-21`) have **no OTel**; `muse` is non-conforming (`MUSE_OTEL_EXPORTER_OTLP_ENDPOINT` instead of `MUSE_OTEL_ENDPOINT`, `muse/src/muse/telemetry.py:401`, = `core/DECISIONS.md` D17). |
| Span-attribute allowlist / redaction at one chokepoint | **YES where instrumented** | `identity/internal/telemetry/telemetry.go:374-388`; `courier/lib/courier/observability.ex:74-80`; `billing/lib/kit/telemetry.rb:137-149`; `muse/src/muse/telemetry.py:113-149` | — | A genuine cafaye strength, and the opposite of Jumpstart's posture. |
| A collector + trace/log/metric backends | **YES, shipped in kit** | `kit/templates/compose/otel-collector.yml` (803 lines: OTLP gRPC+HTTP, syslog/crash, 3 redaction processors, `spanmetrics` connector `:612-653`, fan-out to tempo/loki/mimir); `kit/templates/compose/docker-compose.yml:260,397,423,447,491` | — | Not adopted by the services' own compose files (7 diverged, `kit/templates/parity-allowlist:308-314`). |
| Dashboards | **YES (2, fleet-wide)** | `kit/templates/compose/grafana/provisioning/dashboards/cafaye-fleet-errors.json`, `cafaye-stack-overview.json`; 3 alert rules `.../alerting/rules.yml` | **M** | No per-service dashboard, no status page. Jumpstart has no equivalent. |
| Alert rules | **YES** | `rules.yml:36` (fleet error ratio >5%), `:97` (silent service), `:152` (content-attribute boundary violation) | **S** | **No contact point provisioned, deliberately** (`rules.yml:9-17`). |
| `/metrics` endpoint | **NO — nowhere** | grep `/metrics\|promhttp\|prometheus` across every service → 0 code hits; metrics are *derived* in the collector's `spanmetrics` | **M** | Erlang OTel SDK has no metrics API at all (`courier/lib/courier/telemetry.ex:57-78`). So no custom counters, no gauges, no business metrics. |
| SLOs / burn-rate alerting | **NO — 3 schemas, 0 implementations** | `core/schemas/telemetry/{slo,slo-windows,slo-metrics}.json` exist; **no `slos/` directory in any repo** | **M** | `core/DECISIONS.md` D26/D28 open. |
| Health / readiness probes | **YES (consistent)** | every service mounts `/healthz` + `/readyz`: `identity/internal/httpapi/httpapi.go:212-213`, `courier/lib/courier_web/router.ex:35-36`, `billing/config/routes.rb:5-6`, `pantry/src/http.rs:101-102`, `muse/src/muse/main.py:531-552`, `darkroom/src/http.rs:185,196`, `guard/src/index.ts:83-96`, `parlor/src/app/{healthz,readyz}/route.ts` | **S** | **No `/livez` anywhere** (identity's own OpenAPI maps liveness to `/healthz`, `identity/openapi/v1.yaml:2494`). `parlor/readyz` returns `deps: "none"` — checks nothing (`parlor/src/app/readyz/route.ts:4-7`), which core's `probes.schema.json` `minItems: 1` exists to reject. |
| Ops recipes (deploy, seeds, canaries) | **YES (richest in fleet)** | `kit/templates/deploy/{compose.deploy.yml,entrypoint.sh,redact.py}`; 7 Dockerfiles; ~19 test scripts incl. `canary_test.sh` (10 leak shapes), `no_telemetry_in_readiness.sh`; `pantry/bin/fleet-telemetry`; 10 × `bin/prime`; `courier/priv/repo/seeds.exs` | **S** | No Makefile/justfile/xtask anywhere; `billing` and `cafaye-rb` have Rails/gem Rakefiles only. `courier/ops/glitchtip-database.sql` is the only ops SQL. |
| CI enforces the telemetry contract | **NO — disabled everywhere** | `telemetry: 'false'` in `identity/.github/workflows/ci.yml:153`, `billing:330`, `courier:110`, `muse:99`, `caf:105`. **Not one service sets `telemetry: 'true'`.** | **S** | kit's W3C conformance suite never runs against a real service. |
| Dashboard/health UI in-product | **NO** | `identity` "admin" is 3 JSON routes (`internal/httpapi/admin.go:107-109`); no status page in any repo | **M** | Deliberate for the services; → §8.5. |

**Two documentation defects found while measuring, both customer-facing and both wrong:**
- `docs/src/content/docs/observability.md:10-21` states *"No collector is deployed and no
  observability stack is running"* and *"Exactly one service exports any signal at all: muse.*
  *Identity, billing, courier and guard export none."* **Both sentences are false** — three
  services export and kit ships the whole stack. Duplicated at
  `docs/src/content/docs/pilot.md:657`.
- `core/fleet.yml:81-84,116-119,147-150` records `signals: []` for identity, billing and
  courier. All three are wrong.

---

## 5. Errors

| Feature | We have it? | Where | Gap | Notes |
|---|---|---|---|---|
| RFC 9457 `application/problem+json` everywhere | **YES — 6 of 7** | `identity/internal/httpapi/problem.go:122-131`; `courier/lib/courier_web/problem.ex:110-123`; `billing/app/lib/problem.rb:60-73`; `muse/src/muse/api.py:104-113`; `darkroom/src/error.rs:197-207`; `pantry/src/problem.rs:24-36` | **S** | Six independent hand-transcriptions of one prose contract (`core/docs/openapi-conventions.md:44-78`). No shared error *code* — only the schema and the doc. |
| Same envelope shape across services | **YES** | `darkroom/src/error.rs:193-195` names identity's schema and `:411-418` asserts all seven fields present | — | Consistent, just not centralised. |
| One error *tracker* integrated | **PARTIAL — 1 of 13** | `courier/mix.exs:150`; relay + GlitchTip (see §4) | **L** | grep across every `Cargo.toml`/`go.mod`/`pyproject.toml`/`Gemfile`/`gemspec`/`mix.exs`/`package.json` in the fleet → the only hits are `courier/mix.lock:38` and courier tests. **Every other service's 500 is a log line and nothing else.** |
| Browser / JS / LiveView error capture | **NO — nowhere** | grep `ErrorBoundary\|window.onerror\|unhandledrejection\|componentDidCatch\|Sentry.init\|@sentry` across all repos → **zero matches**; `parlor` has no `global-error.tsx` | **M** | A React render error in production is a blank page. |
| Sanitised 500s (cause in logs, not the body) | **YES — 6 of 7** | `identity/internal/httpapi/problem.go:201-212`; `billing/app/lib/problem.rb:22`; `darkroom/src/error.rs:154-156` (+ test `:423-429`); `pantry/src/problem.rs:91-99`; `courier/lib/courier_web/problem.ex:80` (+ test `health_controller_test.exs:55`) | **S** | **muse is the exception and it loses the cause entirely** — `muse/src/muse/api.py:410-412` returns a fixed string and never logs; grep `logger\|logging` in that file → **zero matches**. A muse 500 leaves no trace of what failed. Worse than leaking, and unique in the fleet. |
| `guard` on the shared envelope | **NO — a second shape** | `guard/src/index.ts:165` `app.notFound` → `{error:"not_found",message:"no such route"}` as `application/json`; `:167-172` `app.onError` → `{error:"internal_error",…}` | **S** | The correct builder exists and is unused on these two paths (`guard/src/problem.ts:72-94`). Self-documented as a `DECISION NEEDED` at `guard/README.md:437-446`. |
| One shared reserved error-code vocabulary | **PARTIAL — divergent** | core's list is prose (`core/docs/openapi-conventions.md:71-73`); identity adds 6 (`identity/internal/httpapi/problem.go:34-75`), courier 2 (`problem.ex:69-82`), billing 3 (`app/lib/problem.rb:31-41`); `pantry` adds `method_not_allowed` and records it as open (`pantry/src/problem.rs:60-64`) | **M** | `rate_limited` is declared by 3 services and enforced by **none** outside `guard`. `muse` maps 405 → `not_found` (`muse/src/muse/api.py:336`) — arguably a lie. |
| Panic handler / catch_unwind | **PARTIAL** | `identity/internal/httpapi/recover.go:18-48`; `courier/lib/courier_web/plugs/telemetry.ex:145-154`; `billing/lib/middleware/request_telemetry.rb:82-95` (observes then re-raises). **None in `pantry`, `darkroom`, `guard`, `muse`, `parlor`** | **S** | Frameworks install defaults; the fleet adds nothing in 5 services. |
| Rate limiting | **PARTIAL — guard only** | `guard/src/middleware/rateLimit.ts` + `rateLimitRedis.ts`; probes exempt by name `guard/src/index.ts:68,137-140`; per-process unless `REDIS_URL` (`guard/README.md:475`, recorded at `guard/cafaye.yml:77`) | **M** | grep in `billing/app` → 0. billing's `/v1` is `security: []` (`billing/openapi/v1.yaml:87`) — **no authorization at all** on the money service (`billing/README.md:171-181`). |
| Circuit breaker | **YES — muse only** | `muse/src/muse/breaker.py:187-202`; `muse/src/muse/errors.py:386` | **M** | None in the other 12. |
| Request timeouts | **NO** | no `TimeoutLayer`/`tower_http::timeout` anywhere; only probe-path timeouts (`courier/lib/courier/health.ex:29`) | **S** | |
| Graceful shutdown / SIGTERM | **PARTIAL — 5 of 13** | `identity/cmd/identity/main.go:119`; `pantry/src/main.rs:54`; `darkroom/src/main.rs:77`; `courier/lib/courier/telemetry.ex:377-380`; `caf/cmd/caf/main.go:35`. **Not in `guard`, `muse`, `billing`, `parlor`** | **S** | |
| Observability "ops recipes" (deploy/seeds) | see §4 | | | |

---

## 6. Frontend

**The honest position, stated without softening:** cafaye has **no LiveView anywhere** and
**no finished customer-facing application**. It has three HTML surfaces, one of which is a
template with eight pages, one of which is a documentation site, and one of which is a
31-line unstyled Go form inside a service that is otherwise JSON-only.

### 6.1 What exists

| Surface | Evidence | Character |
|---|---|---|
| `parlor` — Next.js 16 App Router app shell | `parlor/package.json:24-26`; 8 pages: `src/app/{page,login,register,accounts,invitations/[token],billing/plans,billing/customers}.tsx` + `accounts/[accountId]`; 2 probe routes | **A template.** `parlor/README.md:12-13` — *"This is a template, not a platform-as-a-service dependency."* Deliberately **not registered** in pantry: `pantry/registry/index.yml:359-368`, `blockedBy: schema`, *"It is an app shell, not a platform service."* |
| `docs` — Astro 7 + Starlight static site | `docs/astro.config.mjs:18-24`; 30 MD/MDX pages in `src/content/docs/`; `docs/README.md:73-74` — *"no adapter, no server function, and no environment variable read at runtime"*; has `pricing.md` and `index.mdx` | **Live and real.** The only marketing-adjacent pages in the fleet. Search off (`astro.config.mjs:42`), no analytics (`README.md:277`), no CI workflow at all (`.github/workflows/` contains no files; self-documented at `src/content/docs/pilot.md:666-669`). |
| identity OIDC sign-in page | `identity/internal/httpapi/oidc.go:854-884` (31-line `html/template`, password + TOTP steps); served `GET,POST /oidc/login/{requestID}` `:130-131`; the only `text/html` in any OpenAPI document in the fleet (`identity/openid/openid.yaml:592,677,695,713`) | Honest about itself: *"There is no branding, no 'forgot your password' link and no consent screen"* (`oidc.go:846-847`). |

**Verified absences:** no `.heex`/`.leex` anywhere; `Phoenix.LiveView` absent from every
`mix.lock` (the one hit is an optional peer of `sentry`); no template engine in either
Rust `Cargo.toml`; the only `.eex` files in the fleet are courier's 8 mail templates; the only
`.erb` files are billing's 2 ActionMailer layouts.

### 6.2 Feature table

| Jumpstart feature | We have it? | Where | Gap | Notes |
|---|---|---|---|---|
| Component library / design system | **PARTIAL — 6 hand-rolled components** | `parlor/src/components/ui/{button,input,field,select,state}.tsx` + barrel; `src/components/shell/header.tsx`; 8 pieces exported from `state.tsx` | **L** | Jumpstart documents **13** component pages + Tailwind Forms plugin + Tailwind Plus recommendation (`/docs` nav). cafaye explicitly schedules shadcn and forbids the install without approval (`parlor/AGENTS.md:387,398`). |
| Design tokens / theming (light + dark) | **PARTIAL** | `parlor/src/styles/tokens.css:19-44` (CSS-first `@theme`, 50–950 ramp, semantic layer, reserved status colours). **Self-declared placeholder:** `:4-7` *"PLACEHOLDER PALETTE… it is NOT the cafaye brand."* | **M** | Jumpstart ships light+dark defaults (`/themes`) and replaceable branding (`/branding`). |
| Tailwind pipeline | **YES** | `parlor/postcss.config.mjs`, `src/app/globals.css` | — | Tailwind v4, CSS-first. No `tailwind.config.js` anywhere (v4 style). |
| Icon set | **NO** | none in `parlor` | **S** | Jumpstart recommends Heroicons (`/icons`). |
| Signup screen | **YES** | `parlor/src/app/register/page.tsx` | **S** | No email verification step (identity doesn't send). |
| Login screen | **YES — and it does not work** | `parlor/src/app/login/page.tsx` | **M** | Self-documented: `parlor/README.md:313-319` + `parlor/e2e/edge.conf:5-15` — the browser calls identity cross-origin, identity serves no CORS headers, `OPTIONS /v1/session` answers 405, so **the sign-in form renders "Something went wrong" on a stack that came up green**. 377 unit tests cannot see it because each injects a stub transport. |
| Forgot-password / reset screen | **NO** | — | **M** | Blocked by §3: no mailer. |
| Email verification screen | **NO** | — | **M** | Blocked by §3. |
| MFA enrollment screen | **NO** | — | **M** | identity has a complete MFA API; there is no UI for any of it. |
| OAuth "continue with…" buttons | **NO** | — | **M** | Blocked by §2: no routes. |
| Teams / account switcher | **PARTIAL** | `parlor/src/app/accounts/page.tsx`, `accounts/[accountId]/page.tsx`, `invitations/[token]/page.tsx`; clients `src/lib/accounts.ts`, `src/lib/roles.ts` | **M** | Read + redeem invitation. No per-member management, no role change UI (`parlor/README.md:379-408`). |
| Billing: pricing page | **YES** | `parlor/src/app/billing/plans/page.tsx` | **S** | |
| Billing: checkout / upgrade / downgrade / cancel | **NO** | `parlor/src/lib/billing.ts:195-200` covers 5 endpoints, **none of them `/v1/subscriptions*`** | **M** | **Correction to a subagent's claim, verified myself:** billing **does** have these routes (`billing/config/routes.rb:44-49`) and **does** publish them (`billing/openapi/v1.yaml:357-611`, `info.version: 1.3.0`). The screens are buildable today. `parlor/src/lib/billing.ts:20`'s comment — *"There is no `/v1/subscriptions` here or anywhere else in the document"* — is **stale and false**; it describes billing 1.0.x. → packet p-07. |
| Billing: customer management | **YES** | `parlor/src/app/billing/customers/page.tsx` | **S** | |
| Billing: invoices, usage, seats, dunning | **NO** | billing has **6 tables** and none of them is invoices/usage/credits (`billing/db/schema.rb:17,31,43,57,80,94`); no `quantity`/seats column; stated at `billing/openapi/v1.yaml:74-76` and `billing/CHANGELOG.md:1007-1010` | **L** | Jumpstart: subscriptions, one-time payments, free/enterprise plans, trials, promo codes, embedded checkout (`/billing`, `/subscriptions`, `/one_time_payments`). cafaye: subscriptions only, Stripe only, no seats/trials/invoices/usage. |
| Admin area with dashboards | **NO** | — | **L** | Jumpstart ships `/admin` + madmin per-model dashboards + a revenue/user dashboard + impersonation + 404-for-non-admins (`/admin`). cafaye has **no equivalent in any repository**. |
| Announcements / "What's New" | **NO** | — | **M** | Jumpstart feature (`/announcements`). |
| In-app notifications UI | **NO** | — | **M** | Jumpstart: navbar menu via ActionCable (`/notifications`). |
| API tokens screen | **NO** | identity has 6 scoped-key endpoints; no UI | **M** | Jumpstart has one (`/api`). |
| Settings / profile screens | **NO** | — | **M** | |
| Marketing site / landing page | **PARTIAL** | `docs/src/content/docs/{index.mdx,pricing.md,pilot.md}`; `parlor/src/app/page.tsx:3-11` — *"A directory rather than a pitch. Every link here goes to a screen that exists in this build"* | **M** | **No dedicated marketing site exists** — `docs` is the closest, and it says search/analytics/blog are deliberately absent (`docs/README.md:271-279`). |
| Hotwire Native mobile app backend | **NO** | — | — | Jumpstart ships an example (`/docs/existing_apps`, `github.com/jumpstart-pro/example-hotwire-native-rails-backend`). Out of scope; not counted. |
| `letter_opener` mailbox | **NO** | `courier/config/dev.exs:81-82` | **S** | Neither product documents one. |
| Sidekiq Web UI for admins | **NO** | `courier/AGENTS.md:10-12` | — | → §8.5. |
| LiveView components | **N/A** | — | — | → §8.3. |
| Error boundary / JS error capture | **NO** | zero matches fleet-wide | **M** | See §5. |
| Browser SDK / RUM / web vitals / session replay | **NO** | — | **L** | Not a Jumpstart feature. Listed for completeness. |
| Accessibility floor | **PARTIAL** | `parlor/src/components/ui/button.tsx:12-25` (accessible name must not change on submit; `disabled` + `aria-busy` instead) | **M** | The one place cafaye shows deliberate frontend craft. |

### 6.3 Client libraries (brief — deliberately deprioritised by the PO)

I searched for the stated deprioritisation and **could not find it.** grep
`deprioritis|deprioritiz` across the whole workspace → 2 hits, both unrelated prose in
`kit/tests/staleness.py:761`. The deprioritisation is visible only as *absence of registry
status*, not as a recorded decision:

| | State | Evidence |
|---|---|---|
| `cafaye-ts` | Working. 53 generated operations across 6 services, **0 runtime dependencies**, types from 6 vendored OpenAPI documents with sha256 pins. `README.md:3,16-24`; enforced by `test/no-runtime-dependencies.test.mjs`. | Registered as `kind: cli` (`pantry/registry/index.yml:278-282`) — which is **`pantry` D1, open**. |
| `cafaye-rb` | Working but **not a service SDK** — JWKS token verification + transactional outbox writer. No HTTP client for identity. Its manifest declares `exposes` absent (`cafaye-rb/cafaye.yml:9-14`). | `blockedBy: library` (`pantry/registry/index.yml:433-449`); repo is **private**, so CI cannot clone it — a real coverage gap. |
| `cafaye-py` | Working, **hand-written**, identity only: 20 operations, sync + async, 1 dep (`httpx`). `README.md:14-15`. | **No `cafaye.yml` exists** — `blockedBy: no-manifest` (`pantry/registry/index.yml:506-529`), and pantry's D2 notes the value overstates it because there is no repository to clone. |

They do not dominate this report and, correctly, they should not: none of them is on the
critical path to a working product.

---

## 7. Prioritised packet list

Ordered so each is **independently shippable**. "Runs" = worker-runs of real work
(S ≈ 1, M ≈ 2–3, L ≈ 4–6+). Nothing here is padded; small things are labelled small.

| # | Packet | Area | Repo | Why it is next | Files it likely touches | Deps | Size |
|---|---|---|---|---|---|---|---|
| **1** | **`courier-http-send`** — a `POST /v1/messages` send endpoint | courier | `courier` | courier has a working SMTP path, 8 templates, and **no way in over HTTP**. This is the single blocking dependency of packet 2 and of every email-shaped screen in `parlor`. | `lib/courier_web/router.ex`; new `controllers/message_controller.ex`; `lib/courier/deliver.ex:100-120`; `openapi.yaml` (`info.version` bump); `cafaye.yml:84` `consumes`; contract test in `test/courier_web/openapi_document_test.exs` | none | **M** |
| **2** | **`identity-mailer-wiring`** — replace `recovery.Unavailable{}` with a courier client | identity | `identity` | Unblocks password reset, email verification, email change and **team invitations by email**, and retires the invitation-token-in-the-201-body workaround (`accounts.go:480-491`). `identity/DECISIONS.md:657-660` says the seam makes this *"three lines in `buildRecovery`"* — the packet is small; the effects are not. Requires flipping `cafaye.yml:188` ("identity is a leaf"). | `cmd/identity/main.go:471-498`; `internal/recovery/recovery.go:260-296`; new `internal/mailer/`; `internal/httpapi/accounts.go:478-491`; `cafaye.yml` `consumes`; `README.md:1189-1204` | **1** | **M** |
| **3** | **`identity-oauth-routes`** — mount the routes for the library that exists | identity | `identity` | `README.md:1266` claims OAuth ships; `connected_accounts` is dead schema; `internal/oauth/provider.go:149` builds a `RedirectURI()` that nothing serves; `internal/oauth/client.go:35` points at `internal/httpapi/oauth.go`, **a file that was never written**. This is a lie in the README and in the manifest (`cafaye.yml:29`). | new `internal/httpapi/oauth.go`; `internal/httpapi/httpapi.go:197` (`newMux`); `openapi/v1.yaml`; `README.md:1266`; `cafaye.yml:29`; router-walk test | none | **M** |
| **4** | **`parlor-bff-session`** — make login work against a real stack | frontend | `parlor` | The app cannot log a user in against real services (`README.md:313-319`), so **every** other `parlor` packet is unbuildable-proof. Move to a same-origin BFF or adopt the existing `e2e/edge.conf` proxy in the deploy path. | `src/lib/identity.ts:33,219-243`; `src/lib/token-store.ts`; `src/lib/auth.tsx`; `e2e/edge.conf`; `cafaye.yml`; `README.md:151-172` | none | **S** |
| **5** | **`guard-problem-envelope`** — put 404/500 on the shared envelope | errors | `guard` | The only service in the fleet returning a second error shape; the correct builder is already written and unused on those two paths. Self-documented as a `DECISION NEEDED`. | `src/index.ts:165-172`; `src/problem.ts:72-94`; `src/index.test.ts:80,97`; `cafaye.yml:55`; `README.md:437-446` | none | **S** |
| **6** | **`docs-truth-pass`** — fix the observability docs and `core/fleet.yml` | observability | `docs` + `core` | Two customer-facing pages tell buyers there is no observability when kit ships a full LGTM stack and three services export traces; `core/fleet.yml` records `signals: []` for three services that do. Highest value-to-cost in this list. | `docs/src/content/docs/observability.md:10-21`; `docs/src/content/docs/pilot.md:657`; `core/fleet.yml:81-84,116-119,147-150`; `core/docs/observability.md:337-354` | none | **S** |
| **7** | **`courier-local-mailbox`** — a readable local capture | courier | `courier` | Today nothing reads a rendered message, so courier's own dev loop is blind. `Swoosh.Adapters.Local` is already selectable (`mailer_adapter.ex:133-134`); it needs a store and a dev-only read endpoint. | `config/dev.exs:81-82`; `lib/courier/mailer_adapter.ex`; new mailbox module + dev-only route; `AGENTS.md` | **1** | **S** |
| **8** | **`parlor-subscription-and-members`** — checkout, upgrade/downgrade/cancel, member management | frontend | `parlor` | **billing already publishes every endpoint needed** (`billing/openapi/v1.yaml:357-611`) — this is client work plus screens, not a billing packet. Also retire the stale comment at `src/lib/billing.ts:20`. | `src/lib/billing.ts`; `src/app/billing/**`; `src/lib/accounts.ts`; new pages; `e2e/` | **4** | **M** |
| **9** | **`error-tracking-rollout`** — port courier's relay + GlitchTip to the other services | errors | fleet | 1 of 13 services has error tracking, and it is off by default. The pattern is already built, tested and licence-clean (GlitchTip is MIT), so this is copying a known-good design, not inventing one. Recommend identity + billing first (the two with user-facing 500s). | per service: dependency + init + relay chokepoint + `bin/`-level test; `kit/templates/deploy/reference/courier.deploy.yml` as the exemplar | **1** (courier's relay is the source) | **L** |
| **10** | **`otel-rollout`** — instrument `pantry`, `darkroom`, `guard`, fix `muse` | observability | fleet | 3 of 13 export; 2 Rust services and `guard` export nothing; `muse` reads the wrong env var (`MUSE_OTEL_EXPORTER_OTLP_ENDPOINT`, `muse/src/muse/telemetry.py:401` = core **D17**, open). Also flip `telemetry: 'true'` in at least one CI so kit's conformance suite runs against something real. | `pantry/Cargo.toml` + `src/http.rs:253-272`; `darkroom/Cargo.toml` + `src/observability.rs`; `guard/package.json` + `src/index.ts`; `muse/src/muse/telemetry.py:401`; 5 × `.github/workflows/ci.yml`; `core/DECISIONS.md` D17 | **6** | **L** |
| **11** | **`metrics-and-slos`** — business metrics + SLOs | observability | core + kit + services | No `/metrics` anywhere; no gauges, counters or business metrics; **3 SLO schemas and 0 implementations**; a stalled outbox is invisible (`courier/CHANGELOG.md:330-332`). Erlang's OTel SDK has no metrics API, so courier needs a different mechanism from the others — that decision belongs in this packet. | `core/schemas/telemetry/slo*.json` (adopt, don't rewrite); `kit/templates/compose/otel-collector.yml:612-653`; per-service; `core/DECISIONS.md` D26/D28 | **10** | **M** |
| **12** | **`billing-parity`** — invoices, usage metering, seats, trials, a second processor | courier-adjacent / frontend | `billing` | The largest single feature gap vs Jumpstart's billing docs. **Note honestly:** 6 tables exist and none is invoices/usage/credits; `/v1` has **no authorization at all** (`openapi/v1.yaml:87`), which is arguably a bigger problem than missing features and should be sequenced first inside this packet. | `db/schema.rb`; `app/services/processor/`; `config/routes.rb`; `openapi/v1.yaml`; `README.md:171-181` | **8** (screens depend on it) | **L** |

**Deliberately not in the list, and why:**
- *Outbox publishers* (`identity/main.go:334-338`, courier's `NatsPublisher.Noop`) — real gaps,
  but each needs an architecture decision (which bus?) that is the manager's, not a packet's.
  Fold into packet 11 or raise as `DECISION NEEDED` first.
- *shadcn/ui in `parlor`* — `parlor/AGENTS.md:387` requires manager approval for the
  dependency. Not mine to schedule.
- *`billing` authorization* — called out above; it is the first thing packet 12 should do.

---

## 8. Deliberate divergences

A gap that exists because cafaye chose a different architecture is **not** a gap. Four.

### 8.1 `current_account` scoping and `acts_as_tenant` → cafaye scopes per-request, from the path

- **Jumpstart:** `Current.account_user`, `current_account.projects`, optionally
  `acts_as_tenant :account` as a net for forgotten scoping (`/multitenancy`, `/roles`).
- **cafaye:** no current-account concept exists in any of 16 tables. The `accounts` claim on a
  token is deliberately **an array**: *"An array and not a single `account_id` because a user of
  a cafaye product is in a personal account and usually several team accounts, and a token
  carrying one of them would be wrong for the others"* (`identity/README.md:839-843`).
  Scoping is derived from the path per request.
- **Who:** identity's own design, argued in `identity/migrations/00006_account_users.sql:9-15`.
- **Still load-bearing?** **Yes.** It is what lets one session act in several accounts with no
  switch round-trip — the property `guard` and `parlor` both rely on.
- **Recommendation: keep it.** A switcher would add a stateful concept and a class of bug
  (wrong-tenant writes) for a cosmetic gain. If a UI switcher is ever wanted, build it as a
  *display* concern over an array, not as a stored current account. Do not port `acts_as_tenant`.

### 8.2 Per-account roles as an ordered 3-value enum, not an open role table

- **Jumpstart:** one `admin` role by default, *"you can add more roles in
  `app/models/account_user.rb`"* (`/roles`) — an open, developer-defined set.
- **cafaye:** `owner`/`admin`/`member` as a closed ordered enum, with `AtLeast`/`rank()` and
  fail-closed on an unknown value (`identity/internal/accounts/accounts.go:157-205`; DB enum
  `migrations/00005_accounts.sql:25`).
- **Worth stating that the fleet already knows about Jumpstart's model.** The comment
  justifying the total order names it explicitly —
  `identity/internal/accounts/accounts.go:181-183`: *"A set (Jumpstart's `store_accessor :roles`
  with admin?/owner? predicates) means 'owner but not admin' is expressible, and then every
  check has to decide what that combination is allowed to do. A total order means the question
  has one answer, which is what makes the authorization matrix writable as a table."* This
  divergence was **argued against Jumpstart deliberately**, not by omission, and the same
  argument is repeated in `migrations/00006_account_users.sql:9-15`. Permissions are not a table; they
  are implied by route minimum + a fail-closed route→scope table
  (`internal/httpapi/accounts.go:140-207`).
- **Who:** identity.
- **Still load-bearing?** **Yes** — it is what makes the authorization matrix generatable from
  chi's own route table (`internal/httpapi/authz_matrix_test.go:135-325`).
- **Recommendation: keep it.** Custom roles are a *product* feature, not a platform one. If a
  customer needs custom roles, that belongs in the application layer over these three, not in
  identity's table. Cost of flipping: a migration, the matrix test, and the closure that
  `TestEveryRouteIsInTheMatrix` currently buys.

### 8.3 No LiveView anywhere — a JSON-API platform, not a LiveView app

- **Jumpstart:** a Hotwire/server-rendered application (the Rails product); a Phoenix product
  would presumably have been LiveView.
- **cafaye:** zero `.heex`, zero `Phoenix.LiveView` in any dependency, zero `mount/3`. Three
  services explicitly declare API-only: `courier/AGENTS.md:10-12`, `pantry/AGENTS.md:195`
  (*"No UI."*), and `pantry/src/lib.rs:23`.
- **Who:** per-repository, and consistently.
- **Still load-bearing?** **Yes, and it is the fleet's sharpest property.** Every cafaye
  service is a language-appropriate API that any frontend can consume; that is what makes
  `parlor` (Next.js) a legitimate client and `guard` (TypeScript/Bun) possible at all.
- **Recommendation: keep it, and stop treating "no LiveView" as a gap.** The frontend gap is
  *not* "we should write LiveView" — it is "`parlor` is a template, not a product" (§6.2). A
  customer who wants LiveView should be able to write one against `identity/openapi/v1.yaml`;
  note that **12 of identity's routes are absent from that document**
  (`openapi_drift_test.go:126-144`), which is the real obstacle, and it is cheap to fix.

### 8.4 Courier as a standalone service, and notifications as outbound webhooks

- **Jumpstart:** notifications are in-app via the `noticed` gem — Database, Email,
  ActionCable, **Slack, Twilio, Vonage**, plus a navbar menu (`/notifications`).
- **cafaye:** courier is a separate deployable owning email + notification preferences +
  **outbound webhooks** (Standard Webhooks signature, deliveries table, per-endpoint circuit
  breaker — `lib/courier/webhook_deliveries.ex`, `lib/courier/webhooks/signature.ex`). There
  is no in-app notification store and no Slack/Twilio/Vonage.
- **Who:** courier's Phase-1 scope, `cafaye.yml:63-65`.
- **Still load-bearing?** **Yes.** Standalone courier is why `guard` can be TypeScript and
  `pantry` can be Rust without either owning an SMTP client. The webhook surface is
  *strictly more* general than Slack/Twilio.
- **Recommendation: keep the architecture; close one real hole.** What is missing is the
  **in-app notification model** (a `notifications` table + read endpoint) so a product can
  build the navbar menu that courier's webhook fan-out would then deliver. That is a courier
  packet, and it is the honest reading of the §3 row.

---

## 9. What I could not verify

This section is long because the honest answer is that a lot could not be verified. Ordered by
how much it should change your decisions.

### 9.1 The parity bar itself — the biggest unverified thing in this report

1. **I could not read the Phoenix Jumpstart Pro's documentation, at all.** `jumpstartpro.com`
   and `docs.jumpstartpro.com` are a parked, for-sale domain (§1.1). Therefore **every gap
   size in §2–§6 is measured against Jumpstart Pro *Rails*, not against the named bar.**
2. **I could not verify that a Phoenix/LiveView "Jumpstart Pro" ever existed**, let alone what
   it shipped. I found no GitHub account, no Hex package, no npm package, no third-party
   boilerplate catalogue entry, and no Wayback snapshot. If the Phoenix product existed, it
   was closed-source, short-lived, or renamed — and I cannot tell you which.
3. **I could not verify whether the two same-named products are related.** `jumpstartpro.com`
   and `jumpstartrails.com` may be one team or two. I found no statement either way.
4. **Consequence: the *directions* in this report are safe; the *sizes* are provisional.** The
   findings that do **not** depend on the proxy are: cafaye sends no email (p1), has no
   finished customer app (§6), has no OAuth routes (§2), has error tracking in 1 of 13
   services (§5), and documents its own observability falsely (§4). Those would stand against
   any plausible Jumpstart.

### 9.2 Jumpstart feature-level gaps I could not resolve

5. **Whether Jumpstart supports two-factor auth.** Zero hits across 50 pages for `two-factor`,
   `2fa`, `totp`. I could not determine whether it is absent or merely undocumented.
6. **Which OAuth providers Jumpstart supports.** The docs show configuration by provider name
   and use `twitter` in an example; no list. I did **not** assume "GitHub, Google, Discord…".
7. **Whether Jumpstart has any local mail preview.** No mention of `letter_opener`,
   `swoosh`, `premailer` or `blazer` in 50 pages — but absence of documentation is not
   absence of feature.
8. **Whether Jumpstart ships a marketing site**, and if so what is in it. Not documented.
9. **The exact contents of Jumpstart's admin area** beyond what `/admin` states. No component
   list, no screenshot inventory I could verify.
10. **Whether Jumpstart's `Pay` gem integration has equivalents I overlooked** for usage-based
    billing. `/billing` was truncated at 4,500 characters in my fetch; Paddle, Paddle Billing
    and Braintree were visible, and **the list may continue past where I stopped reading.**
    I did not verify Stripe/Paddle/Braintree is the *complete* processor list.

### 9.3 cafaye-side items I could not fully verify

11. **The PO's stated deprioritisation of `cafaye-rb`/`cafaye-ts`/`cafaye-py` does not exist in
    writing anywhere I could find.** I grepped the whole workspace. The deprioritisation is
    inferable only from registry status (`blockedBy: library`, `blockedBy: no-manifest`) and
    from `pantry` D1/D2. If it matters that these are deprioritised, it should be written down.
12. **Whether `parlor`'s broken login is still broken.** I verified the code path and the
    self-documentation but did not stand up the stack and click it. The claim is
    self-reported by the repository, not independently reproduced by me.
13. **`identity`'s "12 undocumented operations"** — I read the drift test's pinned list
    (`openapi_drift_test.go:126-144`) rather than re-deriving it from the router. The router
    has 49 (method, path) pairs per the subagent count; I did not independently count them.
14. **Coverage of `muse`, `darkroom`, `guard` at source level.** I have their deps, probe
    routes, error paths and (for guard) a confirmed contract violation, but I did not audit
    their business logic. Nothing in this report depends on that.
15. **`kit`'s template-to-service adoption rate.** I counted 5 `telemetry: 'false'` and a
    7-file compose divergence; I did not measure every service's compose against
    `kit/templates/compose/docker-compose.yml` line by line.

### 9.4 What I did verify, and how, so the empty space above is not habit

For the record, the following were **verified in source by direct reading**, not inferred:
identity's entire 49-route table and 16-table schema; identity's absence of OAuth routes
(three independent checks: no `oauth.go` in `internal/httpapi/`, no route registration, no
non-test constructor call); courier's absence of a send route (zero regex matches across
`lib/courier_web/`); courier's 8 templates, 3 email types and Oban config (file listing);
`parlor`'s 8 pages and 6 components (file listing); billing's 12 OpenAPI paths including all
six `/v1/subscriptions*` (`billing/openapi/v1.yaml:357-611`); the stale comment in
`parlor/src/lib/billing.ts:20` (read and compared against that contract); `guard`'s two
non-conforming error paths (`guard/src/index.ts:165,167-172`); `identity`'s refusing mailer
(`cmd/identity/main.go:498`, `internal/recovery/recovery.go:290-296`); identity's outbox
non-start (`cmd/identity/main.go:334-338`); the invitation-token-in-body workaround
(`internal/httpapi/accounts.go:478-491`); `identity/DECISIONS.md:657-667` verbatim; the
absence of `goth` and presence of `golang.org/x/oauth2` in `identity/go.mod:18`; courier's
unwired Swoosh preview plug (`config/dev.exs:81-82`); kit's 2 dashboards + 1 alert-rules file
(directory listing); and every Jumpstart citation in §1.5, fetched live on 2026-10-01.

---

## 10. Method notes and limitations of this report

- **No code was written and no gate was touched.** This file is the only artefact.
- **Every "we have it" claim carries `file:line`.** Where a line range is given I read the
  range. Where a subagent supplied a claim I did not personally re-read, I either re-read it
  or marked the surrounding row as relying on that source.
- **One subagent claim I corrected against the source:** a "billing has no `/v1/subscriptions*`
  at all" reading. `billing/config/routes.rb:44-49` and `billing/openapi/v1.yaml:357-611` both
  show otherwise; `parlor`'s comment is the stale artefact. This changed packet 8 from
  "blocked on billing" to "client work only", which is the difference between an M and an L.
- **Search-based absences** are named in the tables so they can be re-run.
- **No sibling checkout outside this worktree was modified.** Per this repository's
  `AGENTS.md`, `gate.yml` was not touched, no floor was moved, no assertion weakened.
- **Recommended next step for the manager:** decide §9.1 before spending effort on any packet
  whose size depends on the proxy. Packets 1–7 are sized from cafaye's own code and are
  safe regardless of how §9.1 resolves. Packets 8 and 12 are the ones that would change if
  the Phoenix Jumpstart shipped a materially different billing or LiveView story.
