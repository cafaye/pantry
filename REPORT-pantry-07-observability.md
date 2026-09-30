# REPORT — pantry-07-observability

**Packet:** `pantry-07-observability` · **Branch:** `worker/pantry-07` ·
**Date:** 2026-09-30 · **Author:** worker `pantry-07-observability`

**What this packet is.** A research packet. It commits this report and one
measuring script (`bin/fleet-telemetry`). It implements nothing, adds no compose
file, and changes no service's telemetry.

---

## 0. The headline, because it changes the shape of the job

**The observability stack the owner asked for already exists in this fleet, and
it works.** `kit/templates/compose/` ships an OpenTelemetry collector, Tempo,
Loki, Mimir and Grafana, with provisioned dashboards, a redaction boundary
enforced in the collector, and a canary test that proves the redaction. I
started it, pushed 37,000 real spans through it, queried the results back, and
measured what it costs. That is the answer to question 2 and most of question 3,
and it is not an estimate.

**The gap is not the stack. The gap is that no service is plugged into it.**

> Measured across the nine registered services: **one emits telemetry.** `muse`
> does. The other eight do not. `identity` links the OTel SDK as a Go module
> dependency and opens no span. Not one of the nine reads a `*_OTEL_ENDPOINT`
> contract except `muse`. Not one ships a compose file that names the collector.

So the honest report to the owner is not "here is a design for observability."
It is: **the design is done and the delivery is zero.** Every question below is
answered against the thing that exists, not against a proposal.

**A second finding, which is a licence problem and not a footnote.** The packet
asked me to check licences from source, as MD10 did for gitleaks and trufflehog.
Doing that surfaced this:

> **`muse` is `AGPL-3.0-only`.** `muse/pyproject.toml:6` and `muse/README.md:489`.
> The rest of the fleet that declares a licence at all is MIT.

`kit`'s compose file reasons at length that "AGPL attaches to the Grafana
SERVER, not to the applications it observes, so this is compatible with cafaye
being MIT/Apache." That reasoning is sound — and it is reasoning about a
premise that is false for the one service the redaction boundary exists to
protect. This is not mine to resolve and it is not an observability question,
but the owner sells this code and the manager needs it now. It is §6.1.

---

## 1. The real fleet inventory, measured

### 1.1 What the registry says, and what I checked it against

`registry/index.yml` lists **9 registered services** and **6 excluded**. I read
the list as a hypothesis and walked the workspace. `bin/fleet-telemetry` does
this and is the artifact to re-run:

```
$ CAFAYE_ROOT=../cafaye bash bin/fleet-telemetry
  9 registered, 9 checked out, 0 missing
```

The registry's enumeration is **accurate and current**. That is worth saying
plainly, because the packet warned the honest enumeration "is usually wrong" and
in this case it is right — including the subtler claims, which I spot-checked:

- `cafaye-py` is listed as `blockedBy: no-manifest`, a directory with no
  repository. **This has changed underneath the registry.** `cafaye-py/` now
  contains a git repository (`git log` → *"your current branch 'master' does not
  have any commits yet"*), a `LICENSE`, `mise.toml`, `pyproject.toml`, `bin/` and
  `src/`. It still has no `cafaye.yml`, so `blockedBy: no-manifest` is still
  literally true and the tripwire does not fire — but the row's *stated reason*
  ("no files, no git checkout") is now false. This is the exact failure mode
  `index.yml` itself warns about: *"The tripwire only reports exclusions that
  have gone STALE. It says nothing about whether the rows below are still
  ACCURATE."* Dispatched as a finding, not fixed — see §7.

### 1.2 Telemetry emission, per service — the measurement that matters

Reproduce with `bash bin/fleet-telemetry`. Four independent questions per
service, because "does it emit telemetry" is not one question:

| service | lang | OTel SDK declared | opens a span/logger | reads `*_OTEL_ENDPOINT` | ships a collector stack |
|---|---|:---:|:---:|:---:|:---:|
| `muse` | python | **YES** | **YES** | **YES** | no |
| `identity` | go | **YES** | no | no | no |
| `billing` | ruby | no | no | no | no |
| `courier` | elixir | no | no | no | no |
| `darkroom` | rust | no | no | no | no |
| `guard` | typescript | no | no | no | no |
| `pantry` | rust | no | no | no | no |
| `caf` | go | no | no | no | no |
| `cafaye-ts` | typescript | no | no | no | no |

**1 of 9 emits.** Two readings of the table matter more than the count:

- **`identity`: SDK linked, nothing uses it.** `go.mod:29-32` carries
  `go.opentelemetry.io/otel` and three submodules as `// indirect`
  dependencies. There is no `TracerProvider` in production code. A dependency
  audit would score this service as instrumented. It is not — and this is the
  single most misleading shape a fleet inventory can take, because it looks like
  progress on a dashboard.
- **`muse` is the only service core's D16 contract can act on.** `*_OTEL_ENDPOINT`
  is, per core, the *only* contract between a service and its telemetry
  backend. Eight of nine services cannot be pointed at anything, so
  bring-your-own is not merely unused — it is not expressible for them.

### 1.3 What each service emits today, in its own terms

Telemetry is not the same as a trace id. Every service has *some* correlation
handle, and none of them is telemetry:

| service | what it has | is it telemetry? |
|---|---|---|
| `muse` | real OTel spans, `ALLOWED_SPAN_ATTRIBUTES` allowlist, `record()` chokepoint, `Secret` type refused by type | **yes** |
| `identity` | `X-Trace-Id` header + `traceMiddleware`, capped at 200 chars inbound | no — a header, never exported |
| `darkroom` | `src/observability.rs`: task-local `TraceContext`, `tracing` + `tracing-subscriber` (json) | no — structured logs to stdout, no exporter, no OTel |
| `pantry` | `traceparent` in/out, `trace_id` in every problem body | no |
| `courier` | `CourierWeb.Plugs.Trace`, `telemetry_metrics` (a dep, not wired) | no |
| `billing` | `RequestTraceId` concern, `trace_id` in problem JSON | no |
| `guard` | problem JSON with `trace_id` | no |
| `caf` | nothing | no |

`darkroom` is the near miss worth naming: it has a real `tracing-subscriber`
with JSON output and a properly designed task-local trace context. It has the
instrumentation and not the *transport*. Adding an OTLP exporter to it is a
small change — and it is the cheapest first win in the fleet.

### 1.4 The crash layer is the one thing that needs no per-service work

`kit`'s collector takes a `syslog/crash` receiver (RFC5424) and every service
container sets Docker's `syslog` log driver. **Every one of the six languages
already writes a fatal error to stderr**, so a panic becomes a log record in
Loki with zero SDK and zero new dependency, in any language, forever.

I did not exercise this half — it needs a container to panic, and I did not
manufacture one. It is kit's claim, and kit's `tests/no_telemetry_in_readiness.sh`
covers the neighbouring property. Logged in §7.

---

## 2. Question 1 — "on by default, but users can bring their own or turn off"

> **The trap:** a flag is not a contract. "On by default" and "bring your own"
> are in tension, and a boolean cannot express "works out of the box *and* gets
> out of the way when someone else is already running Datadog".

**The contract is already written, and it is the right one.** It is core's D16,
and it is one variable per service:

> `<SERVICE>_OTEL_ENDPOINT` is the only contract. Its default is the collector
> that ships with `bin/dev`. Unset it and the exporter is a genuine no-op.

The shape that resolves the tension: **the default is a value, not a mode.**
There is no `OBSERVABILITY=on|off` and no `TELEMETRY_BACKEND=builtin|external`.
There is one address. Absent, the service talks to the bundled collector. Set,
the service talks to yours and the bundled one is never consulted. Turning
observability off is `*_OTEL_ENDPOINT=""`, which is not a special case in any
code — it is an unset variable.

### 2.1 Fail loudly or degrade quietly

**Recommendation: degrade quietly, and say so on the surface.**

The argument is not a preference. Core's `otel-endpoint.schema.json` already
rats it: a service must declare a **free no-op path** when the endpoint is
unset (`test_unsetting_the_endpoint_declares_a_free_no_op`). And kit's collector
enforces the same shape at the other end — every exporter sets
`sending_queue: {enabled: false}` and `retry_on_failure: {enabled: false}`,
because *"a queue is a memory leak with a telemetry-shaped trigger, and a retry
loop against a dead Tempo is a thread waking on a timer for the life of the
process, invisible in every dashboard."*

Quiet degradation is the right answer **for a platform whose customer chose to
point it at their own collector.** If a self-hoster sets
`MUSE_OTEL_ENDPOINT=https://otlp.example.com` and that host is down, that is
*their* infrastructure being down, discovered by *their* monitoring. Making
muse refuse requests because an observability backend is unreachable converts
their problem into an outage of the product. kit proves the property rather than
asserting it: `tests/no_telemetry_in_readiness.sh` kills the collector and shows
a service still serving, still healthy, still answering requests.

**The one thing that must not be quiet** is the service's *own* failure to
start: if `*_OTEL_ENDPOINT` is set to something malformed, or the SDK is
misconfigured, that is a deployment error and belongs in `/healthz` and the
logs. The line is: **the destination being unavailable degrades; the
configuration being invalid fails loudly.**

### 2.2 What a self-hoster must be able to turn off without losing the services

Everything, by construction, and the reason is a rule rather than an
implementation detail: **telemetry is never in a readiness path.** Not in
`/healthz`, not in `/readyz`, not in a `depends_on`, not in a healthcheck. Six
services keep running with the collector dead. So "turn it off" is not a
migration or a redeploy — it is one variable, and the next request behaves
identically minus the telemetry.

The corollary, and it is the one people get wrong: **turning off the *default*
is not the same as turning off *observability*.** If a self-hoster unsets
`*_OTEL_ENDPOINT` to stop phoning home, they lose the error view too. Their
escape is to point it at *something* they run, not nothing. That is a real
distinction and the README should say it in those words.

### 2.3 Runner-up

**Runner-up: a named-mode variable** — `CAFAYE_TELEMETRY=collector|external|off`.
**It wins if** a self-hoster's environment cannot be templated per service (a
single shared env file across services with different backends), or if core
later wants the *no-op* to be a distinct, testable state rather than the
absence of a variable. **It loses** because it is a second source of truth that
can disagree with the first, and its cost is a fourth value in a contract that
currently has one.

---

## 3. Question 2 — "self hosted Grafana" and what it actually costs

**Grafana reads from somewhere. The answer here is that kit already answers
this**: Grafana is a *view* over Tempo (traces), Loki (logs, and the crash
layer) and Mimir (metrics derived from already-redacted spans by the
`spanmetrics` connector). All three are provisioned as **files**, not clicks, so
`bin/dev` produces a working dashboard with nothing configured, and the
datasource UIDs are stable so a rename in the UI cannot silently break a
provisioned panel.

The stack is not one Grafana. It is **five containers**: the collector plus
four stores, of which Grafana is the cheapest to reason about and the most
expensive to download.

### 3.1 Footprint — method, then numbers

**Method.** I did not estimate. I copied `kit/templates/compose/` to a scratch
directory, ran `docker compose --profile observability up -d --wait` against
kit's own pinned tags, confirmed all eight containers reached healthy, pushed
**37,000 synthetic spans** through the collector's OTLP/HTTP receiver from
inside the compose network, and read the results back out of Tempo and Mimir.
Images were measured with `docker image inspect .Size` after a real pull. Disk
was measured with `du` inside throwaway containers mounting each named volume,
because Docker Desktop's VM does not expose volume mountpoints to the host.

Machine: macOS 26.5.1, 16 GB RAM, Docker 29.4.0, Docker Desktop
(`MemTotal 8393605120` = 7.82 GiB allocated to the VM), `linux/amd64`.

**Images — on disk, uncompressed, after a real pull:**

| image | on disk | role |
|---|---:|---|
| `grafana/grafana:11.3.0` | 455 MB | observability |
| `otel/opentelemetry-collector-contrib:0.115.1` | 254 MB | observability |
| `grafana/loki:3.2.1` | 117 MB | observability |
| `grafana/tempo:2.7.2` | 112 MB | observability |
| `grafana/mimir:2.13.0` | 71 MB | observability |
| `postgres:16.6-alpine` | 254 MB | data |
| `redis:7.4.1-alpine` | 46 MB | data |
| `nats:2.10.24-alpine` | 22 MB | data |

> **Observability images: 1009 MB. Whole stack including the three data
> services: 1331 MB.**

The data services are listed separately on purpose: a service needs postgres
whether or not it is observable, so counting them would flatter the
observability answer.

**RAM — idle, and then under load.** Sampled with `docker stats --no-stream`,
three samples to establish the noise band:

| container | idle RSS | after 37k spans | `mem_limit` ceiling |
|---|---:|---:|---:|
| `grafana` | 155.4 MB | 106.9 MB | 256 MB |
| `otel-collector` | 53.9 MB | 66.5 MB | 384 MB |
| `loki` | 35.1 MB | 85.8 MB | 256 MB |
| `mimir` | 32.5 MB | 56.5 MB | 384 MB |
| `tempo` | 19.1 MB | 95.5 MB | 256 MB |
| `postgres` | 21.4 MB | 21.3 MB | unbounded |
| `redis` | 4.5 MB | 13.2 MB | unbounded |
| `nats` | 4.2 MB | 10.4 MB | unbounded |
| **observability total** | **296 MB** | **411 MB** | **1536 MB** |
| **whole stack** | **326 MB** | **456 MB** | — |

Read those two rows carefully, because the gap between them is the useful part:
**~300 MB idle, ~460 MB under a synthetic 37k-span load, against a declared
ceiling of 1536 MB.** The ceiling is what the stack will *never* exceed; the
measured figures are what it actually used. Tempo's 19 → 95 MB is the honest
shape of a real trace store filling, and it is the number most likely to move
with production traffic. Grafana's idle 155 MB is the largest single consumer
and the least load-sensitive.

**Disk — after 37,000 spans:**

| volume | size |
|---|---:|
| `grafana-data` | 14.0 MB |
| `tempo-data` | 3.5 MB |
| `loki-data` | 28 KB |
| `mimir-data` | 4 KB |
| `nats-data` | 0 KB |
| `redis-data` | 4 KB |
| `postgres-data` | not measured (§7) |

**~17.5 MB of telemetry for 37k spans** — about **480 bytes per span**, all of
it in Grafana's own SQLite (dashboards, users, provisioning state) rather than
in the stores. The stores are empty at this scale because they are block/column
oriented and this is nothing. This number is a *floor*, not a forecast: it does
not bound a week of production traffic, and I did not measure a retention
policy under load.

### 3.2 The answer a self-hoster actually asked

> **"What does this look like when it breaks?"**

~1 GB of images and ~460 MB of RAM under load, for six services. That is
affordable on the modest machine the packet names, and it is the number that
matters: the incumbent alternative is **16 GB** (below). A stack needing 16 GB
to look at six services is not "on by default" for anyone. This one is on by
default for anyone.

`bin/dev` already passes `--profile observability` **by default**
(`templates/bin/dev.sh:89`), so the default path gets the whole stack and a
constrained machine or CI can bring up postgres/nats/redis/collector alone by
overriding one variable. The profile is opt-*out*, which is the correct
direction for a default.

### 3.3 Runner-up

**Runner-up: Grafana Cloud free tier as the default view.**
**It wins if** adoption stalls on the 1 GB download, or if a self-hoster wants
managed alerting without running four stores. **It loses** on the packet's own
constraint — it is a third party on the critical path of a platform sold to
customers who chose self-hosting, and "observability included" would mean
"observability phoning home unless you notice". The self-hosted default is the
product promise; a hosted tier can be an option without being the default.

---

## 4. Question 3 — "collect and see all errors in one place"

**The trap:** traces, metrics and logs are not errors. And the market answer
(self-host a Sentry) is a footprint cliff, not a component.

### 4.1 What an "error" *is* here — the decision, made defensibly

The fleet already answered this in core, and the answer is better than anything
I would have proposed:

> **The predicate for "this is an error" is span status `Error`, not
> `error.type`.** `error.type` is a classification *beneath* that predicate.

So the fleet-wide error view is: **partition by `service.name`, filter on span
status `error`, and use `error.type` as a drill-down dimension *inside* a
service** — valid under a service filter and **never a global grouping key**,
because semconv expects high cardinality there when no filter is applied, and a
global breakdown by class is precisely the wall of ungrouped text the attribute
exists to remove.

`error.type` is a **closed vocabulary of twelve classes plus `_OTHER`**, byte
identical on traces, metrics and logs. Not a message, not an exception class, not
an interpolated value. core-04 originally shipped a `pattern` with a 64-char cap,
which bounded the *shape* and said nothing about the *vocabulary* — so
`user_42_email_invalid` validated cleanly, which on a metric is `tenant_id` on a
measurement under a name that sounds like a classification. It is an `enum` now.

**Why this is defensible as the definition of an error, and why I am not
proposing something else:** the platform has an error *taxonomy* before it has
an error *aggregator*, and the taxonomy is the part that is hard. Grouping,
storage and dashboards are replaceable; "these twelve classes, and this is the
one you page on" is a product decision that gets more expensive every time it is
deferred. It is already made, it is machine-checked
(`test_the_redaction_boundary_is_a_schema`,
`test_the_redaction_policy_never_allowlists_a_content_attribute`), and it is
already enforced identically on all three signals.

### 4.2 Measured: the fleet error view works, end to end

I pushed 7 synthetic services × 2,100 spans (14,700 spans, 20% error status,
five error classes, one `tenant_id` per service) through kit's collector and
queried Mimir with PromQL. Verbatim results:

```
1. services reporting          -> 7
2. errors by service            -> identity 1375 · billing/courier/darkroom/
                                   guard/muse/pantry 1020 each
3. error classes, fleet-wide    -> provider_auth 4975 · conflict 420 ·
                                   dependency_unavailable 420 · internal_error 420 ·
                                   invalid_request 420 · rate_limited 420 · timeout 420
4. classes inside ONE service   -> provider_auth 660 · the other six 60 each
5. distinct tenants in the data -> 1  (resource attribute preserved)
```

And the series Mimir actually held, which is the whole design in one line:

```json
{"__name__":"cafaye_calls_total","error_type":"provider_auth",
 "http_request_method":"POST","http_route":"/v1/route","job":"identity",
 "otel_status_code":"STATUS_CODE_ERROR","service_name":"identity"}
{"__name__":"cafaye_calls_total","http_request_method":"POST",
 "http_route":"/v1/route","job":"identity",
 "otel_status_code":"STATUS_CODE_OK","service_name":"identity"}
```

Two series. The error one carries a class; the success one **does not**, and its
absence is the load-bearing marker — so error rate is computable without putting
a message in a label. `tenant_id` survived **on the resource** and never became a
dimension, which is what keeps a per-tenant total answerable after the
2000-combination cap folds the measurement.

**So: one place to see all errors for the whole system, on one query, across
seven services, today.** It is already built. It has a provisioned dashboard
(`cafaye-fleet-errors.json`) with exactly this shape: a fleet error-ratio stat,
errors-by-service, an error-class table scoped to one service, the crash-layer log
panel, and error spans straight from Tempo.

**And it is showing an empty fleet**, because one of nine services emits.

### 4.3 Recommendation: build the error path out of what the platform already emits

**Do not adopt an error tracker. Use the OTel path that already exists.**

**What it costs: nothing new.** Zero new containers, zero new images, zero new
licences to clear. The stores, the collector, the redaction and the dashboard are
already there and already measured. It is the answer to "we need one place for
errors" that adds no component to a customer's machine.

**What you give up against a real error tracker — stated honestly, because this
is the strongest argument against my own recommendation:**

| capability | a real tracker | the OTel path |
|---|---|---|
| grouping | fingerprint-based, automatic, survives refactors | `error.type` is a *class*; a new failure mode in an existing class is invisible until someone names it |
| stack traces | first-class, symbolicated, aggregated by frame | **prohibited by name.** `error.stacktrace` is dropped by the collector. This is the real cost. |
| release tagging | `release` + regression detection across deploys | nothing; a regression is a rate change you read by eye |
| dedup | collapse 10,000 occurrences into 1 issue | the count is there, the *grouping* is not |
| alerting on new errors | "this is new" is native | manual; a new class is an `internal_error` you notice or don't |
| assignment / workflow | tickets, owners, status | none |

**The stack-trace row is the one that hurts and it is not negotiable.** A stack
trace is exactly where a credential or a prompt ends up — in a local variable
name, a `repr()` of a request body, a SQL fragment. core prohibits
`error.stacktrace` and `error.message` by name for that reason, and the packet's
own redaction constraint forbids it independently. **Any recommendation that buys
grouping by admitting stack traces is recommending the leak.** That trade is not
available to us, which removes most of the reason to want a tracker.

**The grouping gap is the real one and it is fixable inside what we have.** Twelve
classes plus `_OTHER` is a *flat* vocabulary; a tracker fingerprints. The honest
middle is to let a service add a **bounded, low-cardinality dimension** to the
span — the class plus a closed set of discriminators (the dependency that failed,
the provider id) — and let `spanmetrics` carry it as a metric dimension. That
buys most of the grouping, keeps cardinality bounded, and stays on the allowlist
where the collector can enforce it. It is a core change (a new schema field), not
a new component, and it is the single highest-value addition to the error path.

**Runner-up: GlitchTip (MIT), Sentry-API-compatible.**
**It wins if** the grouping and release-tagging gaps above turn out to cost more
than another two containers, or if a customer's ops team already speaks Sentry and
the SDK drop-in is worth more than the licence-cleanliness. **It loses** on three
counts today: it adds a component that must still work in three years; it is a
*Django app plus Postgres plus Valkey*, so it is a fourth datastore for a
platform whose entire thesis is one database per service; and its Sentry
compatibility is the *protocol*, not the SDK, so the drop-in benefit is partial
by construction. It is the right answer to a *different* question — "we need
issue tracking with assignees" — and this is not that question.

---

## 5. Footprint of the alternatives, and the licence table

### 5.1 Self-hosting the incumbent: **no**, and here is the number

Measured from `getsentry/self-hosted/docker-compose.yml` at `master`, parsed
(not regex-counted) for top-level services:

> **53 services.**

| | Sentry self-hosted | kit's stack | GlitchTip |
|---|---:|---:|---:|
| containers | **53** | **5** (observability) | 3 (+1 one-shot migrate) |
| image / RAM | **16 GB RAM + 16 GB swap, 4 CPU, 20 GB disk** — *Sentry's own stated minimum* | ~460 MB under measured load | "as little as 512MB" — *vendor claim, §7* |
| licence | **FSL 1.1 + Apache 2.0** | AGPL-3.0 ×4 + Apache-2.0 ×1 | **MIT** |
| sell the code? | **see 5.3** | yes (config only) | yes |

Sentry's minimum is quoted from `develop.sentry.dev/self-hosted/`, read
2026-09-30: *"4 CPU Cores / 16 GB RAM + 16 GB swap / 20 GB Free Disk Space …
We recommend using 32 GB RAM."* Their own repo README calls self-hosted Sentry
*"feature-complete and packaged up for low-volume deployments and
proofs-of-concept."*

**The answer is no, and the licence makes it a smaller question than it looks.**
Fifteen of Sentry's 53 services are the same `$SNUBA_IMAGE` and twenty are the
same `sentry-self-hosted-local`, so the *process count* is the cost, not the disk.
16 GB to look at nine services is a 35× RAM multiplier over the stack that
already exists and works.

### 5.2 Licence table — every component, quoted from source

**Every cell below was read from the repository named, at the URL shown, on
2026-09-30.** No licence is cited from a blog post, an aggregator, or a
comparison site.

| component | licence | quoted from source | read at | read on |
|---|---|---|---|---|
| `grafana/tempo` | **AGPL-3.0** | "GNU AFFERO GENERAL PUBLIC LICENSE / Version 3, 19 November 2007" | `https://raw.githubusercontent.com/grafana/tempo/main/LICENSE` | 2026-09-30 |
| `grafana/loki` | **AGPL-3.0** | same text | `https://raw.githubusercontent.com/grafana/loki/main/LICENSE` | 2026-09-30 |
| `grafana/mimir` | **AGPL-3.0** | same text | `https://raw.githubusercontent.com/grafana/mimir/main/LICENSE` | 2026-09-30 |
| `grafana/grafana` | **AGPL-3.0** | same text | `https://raw.githubusercontent.com/grafana/grafana/main/LICENSE` | 2026-09-30 |
| `otel/opentelemetry-collector-contrib` | **Apache-2.0** | "Apache License / Version 2.0, January 2004" | `https://raw.githubusercontent.com/open-telemetry/opentelemetry-collector-contrib/main/LICENSE` | 2026-09-30 |
| `getsentry/self-hosted` | **FSL 1.1 → Apache 2.0** (Functional Source License, Apache 2.0 Future License) | "FSL-1.1-Apache-2.0 … Copyright 2016-2024 Functional Software, Inc. dba Sentry … The 'Software' is each version of the software that makes available under these Terms" | `https://raw.githubusercontent.com/getsentry/self-hosted/master/LICENSE.md` | 2026-09-30 |
| `getsentry/sentry-python` | **MIT** | "MIT License / Copyright (c) 2018 Functional Software, Inc. dba Sentry" | `https://raw.githubusercontent.com/getsentry/sentry-python/master/LICENSE` | 2026-09-30 |
| `getsentry/sentry-javascript` | **MIT** | "MIT License / Copyright (c) 2012 Functional Software, Inc. dba Sentry" | `https://raw.githubusercontent.com/getsentry/sentry-javascript/master/LICENSE` | 2026-09-30 |
| `getsentry/sentry-ruby` | **MIT** | "The MIT License (MIT) / Copyright (c) 2020 Sentry" | `https://raw.githubusercontent.com/getsentry/sentry-ruby/master/LICENSE` | 2026-09-30 |
| `getsentry/sentry-go` | **MIT** | "MIT License / Copyright (c) 2019 Functional Software, Inc. dba Sentry" | `https://raw.githubusercontent.com/getsentry/sentry-go/master/LICENSE` | 2026-09-30 |
| `glitchtip/glitchtip` (+ `-backend`) | **MIT** | "MIT License / Copyright (c) 2023 David Burke" | `https://gitlab.com/glitchtip/glitchtip/-/raw/master/LICENSE` | 2026-09-30 |
| `highlight/highlight` | **SPLIT** — Apache-2.0 outside `highlight.io/` and `enterprise/` | "All content that resides under the \"highlight.io/\" directory … is licensed under the license defined in \"highlight.io/LICENSE\". … Content outside of the above … is available under the \"Apache\" license" | `https://raw.githubusercontent.com/highlight/highlight/main/LICENSE` | 2026-09-30 |

Two notes a reader should not have to discover themselves:

- **GlitchTip's GitHub repository is a stub.** `github.com/glitchtip/glitchtip`
  contains one file, `README.md`, and no licence. The licence above is read from
  **GitLab**, which is where the project actually lives. An agent that checked
  GitHub and found nothing would have concluded "unlicensed" — the opposite of
  the truth.
- **Highlight is a split licence**, not a single one. The Apache-2.0 grant is
  explicitly bounded to content *outside* the `highlight.io/` and `enterprise/`
  directories. I did not read those two sub-licences, so I cannot state what
  they are. Since I do not recommend Highlight, this is recorded rather than
  resolved.

### 5.3 The FSL finding, which is the licence question that actually bites

Sentry's self-hosted server is **not open source**. FSL 1.1 is a source-available
licence with two teeth the AGPL does not have:

1. **A permitted-use restriction.** The FSL grants rights *"solely for your
   internal business purposes"* — a **Competing Use** prohibition, defined to
   include offering the software *"in a commercial product or service that
   competes with Sentry's offerings."* A platform that bundles an error tracker
   and sells it is offering a product in a space Sentry sells in. I read the
   clause; I am **not** opining on whether cafaye's use is competition, because
   that is a question for whoever owns the company's licensing position, and the
   honest answer is that I do not know.
2. **A change date.** The grant converts to Apache 2.0 two years after a
   release. So the licence of any given Sentry version is a function of its age,
   which means a licence review of this dependency is not a one-time event.

**This is why MD10's gitleaks/trufflehog ruling was the right precedent to
follow.** The Sentry *SDKs* are MIT and would be perfectly adoptable. The
*server* is the problem, and the server is the part self-hosting requires.

**Recommendation: do not self-host Sentry. Not on footprint, and not on
licence.** The footprint argument alone would have been enough; the licence
argument means that even if the 16 GB were solved, the answer would still be no.

---

## 6. The redaction rule, stated so a component can be tested against it

The packet calls this a hard constraint. It is not a constraint this packet has
to invent — **it is already a schema**, `core/schemas/telemetry/redaction.schema.json`,
enforced at `enforcedAt: "collector"`, and kit's collector implements it. My
contribution is to state it as something an **error aggregator** specifically
can be tested against, because that is the new component and the most dangerous
one in the design.

### 6.1 The rule, in testable form

> **R1 — Default-deny, per signal.** An attribute that is not on the closed
> allowlist for its signal **is not emitted**. The collector's `redaction`
> processor runs with `allow_all_keys: false` before every exporter, on all three
> pipelines. A pipeline with an *empty* allowlist emits no attributes.
>
> **R2 — The allowlist is a list of names, not a filter over values.** A name
> absent from the list never reaches the wire, so a scrubber downstream of the
> leak is not a control.
>
> **R3 — No name on any allowlist may contain a content word.** Prohibited
> substrings: `prompt`, `completion`, `message`, `content`, `text`, `body`,
> `header`, `input`, `output`, `arguments`, `instructions`, `transcript`,
> `query`. This is checkable as a property of the list itself, and core asserts
> it (`test_the_redaction_policy_never_allowlists_a_content_attribute`).
>
> **R4 — `error.message` and `error.stacktrace` are prohibited by name.** Not
> "redacted": **absent.** They are the two attributes a tracing SDK adds by
> default, which is exactly why the prohibition has to be explicit or it ships
> by default rather than by decision.
>
> **R5 — `error.type` is a class from a closed vocabulary of twelve plus
> `_OTHER`.** Never a message, never a stack trace, never an interpolated value,
> never a per-service exception class name. An `error.type` that is not in the
> enum is **dropped**, not truncated.
>
> **R6 — Values are masked, not just keys.** A JWT, an `sk-`/`sk-ant-` prefixed
> key, and a `Bearer <token>` under an *allowed* key are replaced with `****`.
> This is a second, independent barrier: R1/R2 keep a key out, R6 masks a
> credential that arrived under a key we do allow.
>
> **R7 — Span events are not attributes, and the allowlist does not reach
> them.** The deprecated `exception` span event carries its own attribute map at
> a different depth. `exception.message` and `exception.stacktrace` are deleted
> by a dedicated transform; `exception.type` is **kept**, because the exception's
> class is bounded text with no caller content in it.
>
> **R8 — An error aggregator carries no field that is not on the allowlist, and
> has no field of its own.** Grouping keys are drawn from the same closed set.
> A new column in an error store is a new allowlist entry or it is a violation.

**R8 is the one an error aggregator gets wrong by accident**, because the whole
point of an aggregator is to add context. If it adds `service`, `environment`
and `release` as *its own* fields, those fields are outside the boundary the
collector enforces, and the collector cannot help: it has already exported. The
aggregator's context columns are the one part of the pipeline with no upstream
control, which is why they need a gate of their own.

### 6.2 Measured: the boundary holds, against nine leak shapes

I planted a synthetic canary (`CANARY-pantry07-4f2a9c-do-not-ship` — invented
for this measurement, not a real secret, not real customer data) in nine shapes
and read the spans back out of Tempo:

| # | shape | result |
|---|---|---|
| 1 | `prompt: <canary>` | **dropped** (not on allowlist) |
| 2 | `completion: <canary>` | **dropped** |
| 3 | `error.message: <canary>` | **dropped** (R4) |
| 4 | `error.stacktrace: <canary>` | **dropped** (R4) |
| 5 | **canary under an ALLOWED key** (`http.route`) | **SURVIVED** |
| 6 | JWT-shaped value under an allowed key | **masked to `****`** (R6) |
| 7 | `sk-ant-…` key under an allowed key | **masked to `****`** (R6) |
| 8 | deprecated `exception` span event | `exception.message` + `exception.stacktrace` **dropped**; `exception.type` **kept** (R7) |
| 9 | `Bearer <token>` under an allowed key | **masked to `****`** (R6) |

**Row 5 is the one that makes this a measurement rather than a demonstration.** A
collector that dropped everything would pass rows 1–4, 6–9 and *fail* row 5. The
canary surviving under an allowed key is the control that proves the other nine
results are removal, not absence. kit's own `tests/canary_test.sh` makes the same
argument and I ran it: kit's gate is green, and it reports *"a secret in every
leak shape reached no exporter, and the allowed data survived"* plus *"the
spanmetrics connector emitted derived metrics from redacted spans"*.

**Sanitisation, stated plainly.** Everything I quote is synthetic. The canary
string is invented. The JWT, the `sk-ant-` key and the bearer token are
hand-written shapes of the form the `blocked_values` patterns match, not
credentials. The `tenant_id` values are `tnt_<service>`. I read no production
log, no production trace, and no live endpoint, so there was nothing to
sanitise — and I have logged no token, cookie or JWT.

### 6.3 The redaction cannot be delegated to a linter, and what enforcing it cost

The fleet already established that no off-the-shelf static analyser finds a
secret leaked *at runtime* (**MD10**): gosec's `credentials.Match` switches on
four AST node types with **no `*ast.CallExpr` case** — it finds literals, not a
token passed to a logger; Bandit matches `ast.Constant` only; Brakeman's secret
check is optional and off; and of **268 Semgrep taint rules, zero intersect
CWE-532**. I did not re-derive those numbers — they are MD10's, measured on
this fleet, and re-counting them is not this packet's work. I note the direction
they point: a linter finds a secret *written down*, never a secret *shipped*.

So the boundary has to be enforced where the data is assembled. What that meant
in practice, and what it cost:

- **A chokepoint, not a discipline.** One `redaction` processor in one process,
  auditable in one file, that a service cannot bypass by forgetting to
  configure something. The alternative — per-service allowlists as the *only*
  line — has nothing behind it, and core calls that "defence in depth with
  nothing behind it" (D13).
- **The resource/measurement split needed a workaround.** core puts
  `tenant_id` in *both* lists with opposite meanings: prohibited on a
  measurement, required on the resource. The `redaction` processor's
  `allowed_keys` and `ignored_keys` are flat lists applied to both — so
  `ignored_keys: [tenant_id]` would exempt the resource *and* the data point, and
  the data point is the thing the schema exists to prohibit. The fix is a
  stash/restore pair: copy the resource `tenant_id` to `cafaye.stashed.tenant_id`,
  exempt the private name, restore it after redaction. **Order is load-bearing
  in both directions.**
- **A silent-failure class that costs real debugging time.** kit's own comments
  record three: the metrics copy of the allowlist had `cafaye.stashed.*` and the
  traces and logs copies did not, so per-tenant trace and log views were empty
  while only the metric view worked — *"Found by running the stack and reading a
  span back out of Tempo, not by reading this file."* The `_total` suffix rename
  was wired in and documented at length and **never fired on any metric, ever**,
  because a backslash inside a single-quoted YAML scalar is not an escape, so
  OTTL received a regex matching a literal backslash; every dashboard panel
  rendered "No data" against a perfectly working stack. And the metrics signal's
  error predicate was removed by the redaction boundary working *exactly as
  derived from core*, because `spanmetrics` emits a `status_code` label nobody
  had allowlisted.
- **The cost, honestly: this is a large, subtle, hand-written config that fails
  silently and is only correct because someone ran the stack and read a span
  back.** That is the price of making the boundary real rather than aspirational,
  and it is the reason kit's gate has a live canary test rather than a schema
  check. A boundary with no test holds until the first well-meaning change.

**And a finding of my own.** My span reads back show `redaction.redacted.count`
and `redaction.ignored.count` surviving on **traces and logs** as attributes
(kit's config deletes them on the **metrics** signal only, via
`transform/cafaye_metrics_labels`). They carry no content — they are counts — so
this is not a leak. It is a cardinality cost on a trace, and it contradicts the
comment above the processor, which says a list of removed key *names* *"is still
a list you did not choose to publish."* The `summary` default is `info`; at
`debug` the names are published. On traces that setting has no path to suppress
them. **Small, real, and worth a line in kit's report** — it is the same class of
defect kit's config has already produced three times.

---

## 7. What I could not verify

This section is the point of the packet. Everything above is something I ran or
read at the URL and date shown. Everything below is something I did not.

**1. No service in the fleet was started and observed emitting.** I measured
emission *statically* — SDK declared, span opened, endpoint read. I ran kit's
stack and pushed synthetic spans through it, which proves the **stack** works. It
does not prove any service is wired to it, because I did not instrument one. The
finding "1 of 9 emits" is a **static** finding; a service could hold an SDK and
emit through a path my grep does not match.

**2. The crash layer was not exercised.** `syslog/crash` receiving a real panic
is kit's claim. I did not manufacture a panicking container and did not confirm
a crash line arrives in Loki with a `service.name` on its resource. This is the
one error path that needs *no* per-service work, so it is the highest-leverage
unverified claim in this report.

**3. GlitchTip's "512MB" is a vendor claim I did not reproduce.** Read from
`glitchtip-backend/README.md` on GitLab, 2026-09-30: *"GlitchTip runs with as
little as 512MB of ram."* I pulled the image (706 MB uncompressed) and read its
`compose.yml` (postgres + valkey + one `web` container running
`run-all-in-one.sh`, which is web+worker in one process). I did **not** run it
and did not measure its RAM. Treat 512 MB as marketing until someone measures it.
Note the honest complication: GlitchTip's dev compose is not its production
topology, and its `.do/deploy.template.yaml` describes **5** components
(web + worker + migrate + Postgres + Redis), so "3 containers" is the *modern*
shape and 5 is the documented one.

**4. Two images could not be measured.** `clickhouse/clickhouse-self-hosted-local`
and `getsentry/self-hosted` returned HTTP 401 from the registry API on every
attempt (rate limiting or an auth requirement). For Sentry this does not matter —
I did not sum image sizes, because 33 of its 53 services are locally *built*
images (`sentry-self-hosted-local`, `$SNUBA_IMAGE`) that cannot be measured from
a registry at all, and a disk figure built from a partial list would be a
confident wrong number. I used Sentry's own documented minimum instead, which is
a stronger number than a size sum. `clickhouse/clickhouse-server:latest` measured
281 MB compressed as a rough comparable, but it is **not** the image Sentry
pins and should not be quoted as Sentry's.

**5. `postgres-data` volume size was not measured.** The other six volumes were
read with `du` inside throwaway containers. Postgres either refused or the
container had no `du`; I did not chase it. The data services are excluded from
the observability footprint anyway, so this does not affect the headline, but the
table in §3.1 has a hole and it is a hole rather than a zero.

**6. Disk growth is a floor, not a forecast.** ~17.5 MB for 37k spans, of which
14 MB is Grafana's own SQLite. I did not measure a retention policy under
sustained load, did not run for a day, and did not measure what a week of
production traffic costs. **Anyone repeating this must re-measure under real
traffic before quoting a retention number to a customer.**

**7. RAM is one machine, one architecture, one load.** 16 GB macOS, Docker
Desktop with 7.82 GiB allocated to the VM, `linux/amd64`, 37k synthetic spans
pushed in 7 batches over ~2 minutes. A real fleet has more services, more
attribute combinations, longer-running Tempo ingesters and a JVM-free Grafana
that nonetheless grows with dashboard count. Grafana's idle 155 MB is the number
I would expect to move most in production. Treat ~460 MB as *this* measurement,
not as a specification.

**8. The redaction canary used hand-written credential shapes.** The JWT, the
`sk-ant-` key and the bearer token are strings I composed to match the shapes
the `blocked_values` regexes describe. They are not real credentials and not
sampled from a real incident. A real token with a character class the four
patterns do not cover would not be masked — the patterns are four regexes, and
**R6 is only as good as those four regexes.** I did not test for false negatives
from unanticipated token formats. This is the sharpest edge in the whole design
and it deserves a proper adversarial pass.

**9. I did not re-derive MD10's static-analyser numbers.** gosec's missing
`*ast.CallExpr` case, Bandit's `ast.Constant`-only matching, and *0 of 268*
Semgrep rules intersecting CWE-532 are cited from MD10 in the fleet's
`DECISIONS.md`, which states them as measured. I did not re-run Semgrep's
corpus and do not claim independent confirmation. They are load-bearing for the
argument that redaction must be enforced at the boundary, so a reader who doubts
them should re-run that count.

**10. The `cafaye-py` drift finding is a read, not a check.** I observed a `.git`
with zero commits and a set of files where the registry row says "no files, no
git checkout, no `cafaye.yml`". I did not run `tests/schema.rs` to see whether
the tripwire fires — I expect it does not, because the row's machine-checked
claim (the manifest is absent) still holds. If the row's *reason* is now false
while its *claim* is true, that is a gap in `every_exclusion_reason_is_still_true`
and the next pantry packet should decide whether the vocabulary needs a
`planned` value (already `DECISIONS.md` D2).

**11. Two licence questions I am not answering.** Whether bundling self-hosted
Sentry would be "Competing Use" under the FSL is a question for whoever owns the
company's licensing position, and I have read the clause without forming a view.
And Highlight's `highlight.io/` and `enterprise/` sub-licences are unread; the
Apache-2.0 grant is explicitly bounded to exclude them.

**12. kit's gate was run, and pantry's was not yet green.** `bash
tests/validate.sh` in `kit` → **exit 0**, 19/19 self-test breakages red,
canary passed. `mise x -- ./bin/prime` in **this** worktree → **exit 1**, and
the cause is not this packet. See §8.

---

## 8. The gate, reported honestly

`pantry`'s gate is `mise x -- ./bin/prime`. Captured in **bash**, under
`set -o pipefail`, with the gate's own exit code taken from `${PIPESTATUS[0]}` —
zsh has no `PIPESTATUS`, and a piped exit code under zsh is the exit code of
`tail`, which is always zero. That mistake has already produced one false green
in this fleet, so the number below is the collector's, not the tail's.

### 8.1 Pass and skip counts, separately

| suite | passed | failed | skipped |
|---|---:|---:|---:|
| `src/lib.rs` | 0 | 0 | 0 |
| `src/main.rs` | 0 | 0 | 0 |
| `tests/api.rs` | 24 | 0 | 0 |
| `tests/ci.rs` | 6 | 0 | 0 |
| `tests/contract.rs` | 9 | 0 | 0 |
| `tests/drift.rs` | **12** | **2** | 0 |
| `tests/filters.rs` | 13 | 0 | 0 |
| `tests/kind.rs` | 7 | 0 | 0 |
| `tests/manifest.rs` | 13 | 0 | 0 |
| `tests/schema.rs` | 7 | 0 | 0 |
| doc-tests | 0 | 0 | 0 |
| **total** | **91** | **2** | **0** |

> **91 passed · 2 failed · 0 skipped.**

**Zero skipped is the important half of that line.** The drift tests are the
point of this repository and they **ran** — `PANTRY_CAFAYE_ROOT` resolves the
workspace, so all 14 executed against live checkouts. A gate that had skipped
them would have been a green badge over an unrun test, which is worse than no
gate. The 2 failures are the *drift test working*, not the drift test broken.

### 8.2 Why it is red, and it is not this packet

**The gate was already red before I changed anything.** My worktree was clean at
`63f0b83` with an empty `git status` when I ran it. The two failures are
**cross-repository drift**, exactly the failure mode `registry/index.yml`
predicts in its own header — *"pantry's tests read the LIVE filesystem, so any
merge anywhere in the fleet can invalidate this registry."*

`identity` and `muse` have both merged new packets to their own `master` since
pantry's copies were taken, and pantry's verbatim copies are stale:

```
every_registered_entry_is_a_verbatim_copy_of_the_services_own_bytes
  identity  copy 11591 bytes, real 13302 bytes — first differs at line 216
            COMMENT-ONLY drift: the copy is missing decisions recorded since
  muse      copy  2930 bytes, real  6280 bytes — first differs at line 49
            A YAML FIELD MOVED TOO — the copy is not merely stale, it is WRONG

every_registered_entry_matches_the_real_service_on_disk
  muse: dependencies[0].required — copy says false, real says true
        (muse-06 made identity a REQUIRED dependency: every token is verified
         against identity's JWKS, so muse without identity is 503 for every
         request. The real file says so in eleven lines of comment.)
```

The test prints the fix, and it is the right fix:

```sh
cp ../identity/cafaye.yml registry/services/identity/cafaye.yml
cp ../muse/cafaye.yml     registry/services/muse/cafaye.yml
```

**I did not apply it.** This packet commits a report and a measuring script and
nothing else; refreshing two registry copies is pantry's own drift maintenance
and belongs in the commit that caused it, or in a packet dispatched for it.
Applying it here would also have made this diff harder to read, which is the
whole reason a report gets its own commit.

**Per the packet's instruction not to push until the gate is green, I have not
pushed this branch.** The gate is red, and the red is pre-existing and
unrelated to this packet's two files. The manager has two honest options:
dispatch the two-line refresh above (after which this branch is green and
mergeable as-is), or merge the report and land the refresh separately. I am not
going to fabricate a green by loosening the check, narrowing the assertion, or
excluding `muse` — MD10's precedent is that a check weakened to go green is a
check that has stopped checking, and this one has now caught three real problems
in a row.

### 8.3 Did the suite move because of this report?

**No.** Measured before any file was written, and again after both files exist:

```
before (clean tree, 63f0b83):  passed=91 failed=2 ignored=0   GATE_EXIT=<nonzero>
after  (report + script):      passed=91 failed=2 ignored=0   GATE_EXIT=101
```

A `diff` of the per-suite `test result:` lines between the two runs differs
**only in the reported durations** (`finished in 1.80s` vs `1.22s`). The counts
are identical and the failing test names are identical:

```
every_registered_entry_is_a_verbatim_copy_of_the_services_own_bytes
every_registered_entry_matches_the_real_service_on_disk
```

A markdown file and a bash script cannot affect a check that compares bytes in a
sibling checkout against a copy in this one. The suite did not move.

---

## 9. Recommendations, condensed

| # | recommendation | runner-up | the runner-up wins when |
|---|---|---|---|
| 1 | **One variable, `<SERVICE>_OTEL_ENDPOINT`.** Default = the bundled collector; unset = free no-op; unreachable = **degrade quietly**. A named-mode variable (`CAFAYE_TELEMETRY=collector\|external\|off`) | the environment is shared across services with different backends, or core wants no-op to be a distinct testable state |
| 2 | **Keep kit's stack; it is the answer and it costs ~1 GB and ~460 MB.** Grafana reads Tempo + Loki + Mimir, provisioned as files. Do not self-host Sentry: 53 services, 16 GB RAM minimum, and FSL 1.1 is not open source | Grafana Cloud free tier as the default view | adoption stalls on the 1 GB download, or a self-hoster wants managed alerting without running four stores |
| 3 | **Build the error path on what already emits.** Span status is the predicate; `error.type` is a 12-class drill-down; one query, all services, measured working. Add a **bounded low-cardinality dimension** to close the grouping gap — a core change, not a new component | GlitchTip (MIT), Sentry-API-compatible | grouping and release-tagging cost more than two more containers, or a customer's ops team already speaks Sentry |

**And the thing that outranks all three:** eight of nine services emit nothing.
The stack is finished and the fleet is not plugged in. The cheapest first wins,
in order: `darkroom` (has `tracing-subscriber` and a task-local trace context;
needs an exporter), `identity` (SDK already linked, opens no span), then a
`*_OTEL_ENDPOINT` read in each service so bring-your-own becomes expressible at
all.

---

## 10. Files in this packet

| file | what it is |
|---|---|
| `REPORT-pantry-07-observability.md` | this report |
| `bin/fleet-telemetry` | the measuring script. Reproduces §1 and §3.1. `bash bin/fleet-telemetry` for the inventory (no Docker needed); `--footprint` to also pull and measure the images. It is **not a gate** — it asserts nothing and cannot fail a build. |

```
$ CAFAYE_ROOT=../cafaye bash bin/fleet-telemetry
$ CAFAYE_ROOT=../cafaye bash bin/fleet-telemetry --footprint
```

Both paths were run on 2026-09-30 and the output in §1 and §3.1 is verbatim from
those runs. The script prints what it did **not** measure as loudly as what it
did — three bugs in it during development (a dead `grep -q | grep -qv` branch
that reported the one working service as emitting nothing, a `set -e` exit on a
`grep` that matched nothing, and a `#` comment inside a process substitution) are
recorded in the file next to the code that replaced them, because a measuring
script that is quietly wrong is worse than no measuring script.
