# REPORT — pantry-07: observability

**Packet:** `pantry-07-observability` · **Worker:** `pantry-07` · **Date:** 2026-09-30
**Worktree:** `pantry-worker-pantry-07-observability` · **Base:** `92ca41e` (`worker/core-11-pin`)
**Gate:** `mise x -- ./bin/prime` — **114 passed, 0 failed, 0 ignored, 21 skipped** in a pantry-only
clone; **114 passed, 0 failed, 0 ignored, 0 skipped** in the full workspace. Both `GATE_EXIT=0`.

**What I committed:** this report, and `scripts/redaction_canary.py` — the script that produced
the finding in §5. Nothing else. No compose file, no service change, no emission change.

---

## 0. The one-paragraph answer

The platform's telemetry story is not "zero adoption of a stack that does not work". It is
**one service emits spans, and the collector those spans go through has two measured holes that
are exactly on the error path** — a log record's free-text `body` and a span's `status.message`
both reach a retained store verbatim, and both are where a credential or a prompt ends up. Grafana
is already shipped, already correctly licensed, and 281 MB of RAM. The incumbent error tracker is
FSL-1.1, not BSD, and needs 53 containers. The recommendation is therefore: **converge on what is
in `kit`, fix the two holes in it first, and do not add an error tracker at all** — with
GlitchTip (MIT, one container) named as the runner-up and the condition under which it wins.

---

## 1. The real fleet inventory, measured

The brief said to treat any handed list as a hypothesis. Three of the numbers I was given were
wrong, so here is the enumeration and here is the correction.

### 1.1 What exists

Fourteen repositories are on `master` in the workspace, plus one empty directory. Nine are
registered in `registry/index.yml`; six names are in the exclusion record, one of which
(`cafaye-py`) is a directory rather than a repository (`DECISIONS.md` D2).

| repository | language | pantry's answer | `bin/prime` + `mise.toml` | adopts `ci.reusable.yml` | `docker-compose.yml` | OTel dependency | wires an OTLP exporter | declares a `tier` |
|---|---|---|---|---|---|---|---|---|
| `identity` | go | `kind: api` | yes | yes | yes, 52 lines | `go.mod`, **all `// indirect`** | no | no |
| `billing` | ruby | `kind: api` | yes | yes | yes, 44 lines | none | no | no |
| `muse` | python | `kind: api` | yes | yes | yes, 86 lines | `pyproject.toml`, real | **yes** | yes |
| `darkroom` | rust | `kind: api` | yes | yes | yes, 53 lines | none | no | yes |
| `courier` | elixir | `kind: api` | yes | yes | yes, 59 lines | none | no | yes |
| `guard` | typescript | `kind: api` (curated) | yes | yes | yes, 60 lines | none | no | no |
| `pantry` | rust | `kind: api` | yes | **no** (own `ci.yml`) | none | none | no | no |
| `caf` | go | `kind: cli` (curated) | yes | yes | none | none | no | no |
| `cafaye-ts` | typescript | `kind: cli` (curated) | yes | **no** (own 92-line `ci.yml`) | none | none | no | no |
| `docs` | typescript | `blockedBy: library` | yes | yes | none | none | no | no |
| `cafaye-rb` | ruby | `blockedBy: library` | yes | **no** (own 75-line `ci.yml`, private) | none | none | no | no |
| `parlor` | pre-core draft | `blockedBy: schema` | yes | yes | none | none | no | no |
| `core` | spec | `blockedBy: not-a-service` | yes | yes | none | none | no | no |
| `kit` | — (config only) | `blockedBy: no-manifest` | **no, by its own rule** | yes (self) | none | none | no | n/a |

**Corrections to the numbers I was handed:**

- **"Nine services have `bin/prime` and `mise.toml`"** — it is **13 of 14**. `kit` is the only
  exception, and it is deliberate: `kit/AGENTS.md` says "Config only. No runtime code, no
  dependencies, no generated output."
- **"Eight of eight with CI adopt `ci.reusable.yml`"** — it is **11 of 14**. The three that do not
  are `pantry` (it needs its own `workspace-drift` job, and says why in its `ci.yml`), and the two
  libraries `cafaye-rb` and `cafaye-ts`, which each carry a hand-rolled `ci.yml` running
  `bin/prime` directly. That correlation is the interesting part: **the three repositories that opt
  out of the shared CI are the two that are libraries rather than services, and the one that must
  see the fleet.** See `DECISIONS.md` D1 for why the two libraries are not services.
- **"All nine `docker-compose.yml` files have diverged from kit's by roughly 450 lines each"** —
  there are **six** `docker-compose.yml` files, 44 to 86 lines each, and they do not diverge from
  kit's 433-line template so much as **not be it**: they are an app and one database. Comparing
  service names, they share exactly one with the template (`postgres`) in three cases and zero in
  the other three. "Replaced, not drifted" is the right half of the sentence; the counts are not.
  Total files in play, including the template: **seven**.

### 1.2 What it emits today, and where the gaps are

Measured by reading the dependency manifests and grepping the source, not by inference:

- **One service of fourteen emits spans.** `muse`, and only `muse`. `muse/src/muse/telemetry.py:398`
  constructs an `OTLPSpanExporter`.
- **`identity` has the OpenTelemetry Go modules and no implementation.** All four
  `go.opentelemetry.io/*` lines in `identity/go.mod` are marked `// indirect`. There is no
  `TracerProvider`, no `OTLPSpanExporter` and no `OTEL_SDK_DISABLED` anywhere in the repository. It
  is a transitive artefact of something else, not telemetry.
- **Twelve of fourteen declare no OpenTelemetry dependency at all**, including every service the
  observability stack is supposed to make visible.
- **Zero of the seven registered HTTP services expose `/metrics`.** `pantry`'s own `AGENTS.md`
  forbids it ("No health polling of other services… not a probe, not a metrics scrape"), and the
  measurement agrees with the rule rather than the other way round. All seven do answer
  `/healthz` and `/readyz`.
- **Three of fourteen declare a `tier`** (`courier`, `darkroom`, `muse`). `kit/templates/tier/`
  ships implementations for eight languages — `bun`, `elixir`, `go`, `node`, `python`, `ruby`,
  `rust`, and one more — and **none for `typescript`**, which is the language of `guard`, `docs`
  and `cafaye-ts`.

### 1.3 The one contract divergence, and it is in the only service that emits

`kit/templates/AGENTS.md` states the contract:

> **`<SERVICE>_OTEL_ENDPOINT` is the only contract** — `MUSE_OTEL_ENDPOINT`, `CAF_OTEL_ENDPOINT`,
> whatever this service is called, uppercase, no `OTEL_EXPORTER_` prefix and no `_EXPORTER_` infix.

`muse/src/muse/main.py:162` reads:

```python
otel_endpoint=source.get("MUSE_OTEL_EXPORTER_OTLP_ENDPOINT") or None,
```

That is the prefix the template forbids and the infix the template forbids. And `muse`'s own test
docstring at `muse/tests/test_telemetry.py:285` describes the *template's* spelling
(`MUSE_OTEL_ENDPOINT`) as the variable that degrades to "traces are not exported" — so the
repository documents the contract it does not implement. `kit/tests/validate.sh:1920` checks the
template's pattern and would reject muse's, which is presumably why no service has been checked
against it yet.

**Why this is a finding and not a nit.** The contract is the *only* thing that makes bring-your-own
possible: it is the one variable a self-hoster with an existing Datadog or Grafana Cloud sets. Two
of two plausible spellings in the fleet, with the wrong one in the only service that ships, means
the contract has never been exercised by a second implementation. Fix it before anything depends
on it.

---

## 2. Answer 1 — "on by default" and "bring your own" are a contract, not a flag

### 2.1 The trap

The two halves are in tension and both are legitimate. "On by default" means a self-hoster who has
never heard of OpenTelemetry gets working telemetry with zero configuration. "Bring your own" means
a self-hoster with a Prometheus and a Tempo must be able to point at theirs and have the platform
*stop running its own* — not merely add a second collector and split the telemetry in two.

### 2.2 The contract, stated precisely

I recommend this, and it is the boring shape: **one variable per signal family, defaulted to the
shipped collector, with the override being an endpoint URL and nothing else.**

| | the default | the override | what "off" means |
|---|---|---|---|
| **traces** | `http://<collector>:4318` on the compose network, OTLP/HTTP | any OTLP/HTTP endpoint | the variable is unset |
| **metrics** | the same endpoint | any OTLP/HTTP endpoint | the variable is unset |
| **logs** | the same endpoint, **plus** container stderr via the syslog driver | any OTLP/HTTP endpoint, **or** a log store the platform ships | the variable is unset |
| **errors** | not a separate signal — see §4 | — | — |

**The naming.** `<SERVICE>_OTEL_ENDPOINT`, one variable, no infix, no prefix. `muse` must be
changed to it (§1.3). The argument for one variable over four is not elegance: it is that a
self-hoster's failure mode is *setting the wrong one of four*, and `OTEL_SDK_DISABLED` is
per-family in the OpenTelemetry spec, so four variables is four independent ways to phone home.

**What the override actually turns off.** Setting `MUSE_OTEL_ENDPOINT=https://otlp.example.com`
must make the platform's own collector, Tempo, Loki, Mimir and Grafana **stop**, not merely go
quiet for that service. That is a deployment-time decision, not a runtime one, and the honest
mechanism is a compose profile the operator chooses, not an environment variable the operator sets.
This is the part of the request that has no answer in the current design, because nothing in the
fleet currently reads the variable at all except `muse`.

**What a self-hoster must be able to turn off without losing the services.** Three independent
switches, and the invariant is that all three leave the *services* running:

1. `MUSE_OTEL_ENDPOINT` unset → the SDK exporter is a no-op (the OpenTelemetry spec's own
   `OTEL_SDK_DISABLED` plus the three `*_EXPORTER_*` variables, per `kit/templates/AGENTS.md`).
2. The observability compose profile not started → no collector, no backends, no host ports.
3. The Grafana service itself omitted → dashboards gone, `GET /v1/services` untouched, `/readyz`
   green, because **`pantry` must never depend on any of it**.

**Telemetry is never in a readiness path.** Already the house rule and I found no counter-example:
no `depends_on` on the collector, no healthcheck probing an OTLP port, nothing in `/readyz` in any
of the eight services. That rule is load-bearing for exactly the "turn it off" question, and it is
currently correct.

### 2.3 Fail loudly or degrade quietly? — **fail loudly, and only once, at boot**

**Recommendation: fail the container at boot if the endpoint is set and unreachable. Never fail a
request; never retry forever; never warn per request.**

Why, and the cost:

- The failure being guarded against is a self-hoster setting `MUSE_OTEL_ENDPOINT` to a typo and
  getting **no telemetry and no indication why** for a week. That is the worst outcome available,
  because the system looks healthy. `muse` already has the precedent for this reasoning, in the
  other direction: `MUSE_VAULT_KEY` has no default and a missing one is a boot failure, because
  "a vault that starts with a fallback key is a vault whose keys are readable by anyone who has
  read this repository." A telemetry endpoint has no safe default either, once it has been
  overridden — a silent fallback to the shipped collector is a self-hoster's traffic going to a
  box they did not choose.
- The cost of failing at boot is that a telemetry outage becomes an application outage. That is
  acceptable **only because telemetry is not in anyone's readiness path** — which is true today
  and must stay true, and is the whole reason §2.2's invariant matters. A readiness probe that
  touched telemetry would make this recommendation wrong.
- The precise rule: **one bounded reachability attempt at boot** (the OpenTelemetry SDK's own
  timeout, not a retry loop), then exit non-zero with a message naming the variable and the host.
  After boot, **every export failure is silent and dropped** — a collector that goes down at 03:00
  must not take the request path with it. The asymmetry is deliberate: configuration errors are
  loud, runtime errors are quiet, and the line between them is "was the operator watching".

**The measured thing that makes this urgent.** The shipped collector returns **HTTP 200 with
`{"partialSuccess":{}}` to a request carrying 4000 spans that it then silently discards** (§5.4).
Whatever the receive-side contract is, "accepted" and "stored" are different facts and a
self-hoster cannot see the difference. The boot check has to be a *round trip through to the
store*, not a TCP connect to the collector.

---

## 3. Answer 2 — "Grafana" is a dashboard tool, so: what does it read from, and what does it cost?

### 3.1 What Grafana reads from here — measured, not assumed

I brought the stack up and asked it. `GET /api/datasources` and `GET /api/search?type=dash-db`
against `grafana/grafana:11.3.0` with kit's provisioning files present:

```
datasources   loki (type loki)   mimir (type prometheus)   tempo (type tempo)   count = 3
dashboards    "cafaye — every error in the fleet"   "cafaye — local stack"   count = 2
```

So the answer to "what does it read from" is: **it reads from the three stores the same
`docker-compose.yml` starts, over service names on the compose network, and nothing else.**
Tempo at `http://tempo:3200`, Loki at `http://loki:3100`, Mimir at
`http://mimir:8080/prometheus`. There is no cloud account, no `GF_*` credential and no phone-home
path — I verified `GF_ANALYTICS_REPORTING_ENABLED=false` and the container's egress is not
otherwise configured.

And the answer to the follow-up the owner actually asked — "self hosted grafana set up too" — is
that **it already exists, correctly provisioned, with a dashboard whose title is literally
"cafaye — every error in the fleet", and it is running in zero of fourteen repositories.**

### 3.2 The footprint, with method stated

**Method.** I copied `kit/templates/compose/` to a scratch directory, wrote a `.env` moving every
published port into a free block (16100–16107, so as not to disturb a stack another agent had
running), and brought it up with `--profile observability`. All eight containers came up
`Healthy`. I then measured: (a) image sizes from the registry v2 API, walking a multi-arch index to
the `linux/amd64` manifest and summing its declared layer sizes; (b) on-disk size from
`docker image inspect .Size`; (c) RAM from `docker stats --no-stream` at rest and after load, with
the declared caps from `docker inspect .HostConfig.Memory`; (d) volume growth from
`du -sh` on each named volume. The stack was then torn down and its volumes removed.

**Images.** Eight images, one per compose service, every tag pinned.

| image | tag | compressed pull, amd64 | on disk |
|---|---|---|---|
| `postgres` | 16.6-alpine | 109,483,511 B | 254.1 MB |
| `nats` | 2.10.24-alpine | 10,009,883 B | 22.6 MB |
| `redis` | 7.4.1-alpine | 19,442,650 B | 46.1 MB |
| `otel/opentelemetry-collector-contrib` | 0.115.1 | 74,325,863 B | 254.0 MB |
| `grafana/tempo` | 2.7.2 | 55,715,374 B | 112.4 MB |
| `grafana/loki` | 3.2.1 | 37,096,821 B | 117.7 MB |
| `grafana/mimir` | 2.13.0 | 25,795,912 B | 71.4 MB |
| `grafana/grafana` | 11.3.0 | 132,706,750 B | 455.8 MB |
| **total** | | **464,576,764 B (443 MiB)** | **1,334.1 MB (1.30 GiB)** |

The on-disk figure sums `docker image inspect .Size` per image, so any layer shared with another
image on the machine is counted more than once. On a machine holding nothing else it is exact, and
it is the figure I would quote a self-hoster.

**RAM.** This is the number the owner's question turns on, and it is much better than feared.

| | at rest, no data | with 40 spans + 40 logs + metrics resident | declared `mem_limit` |
|---|---|---|---|
| observability five (collector, tempo, loki, mimir, grafana) | 243.4 MiB | 281.1 MiB | **1,536 MiB (1.50 GiB)** |
| data three (postgres, redis, nats) | 30.6 MiB | 40.8 MiB | **none — `HostConfig.Memory == 0`** |
| **all eight** | **274.0 MiB** | **321.9 MiB** | — |

Two things follow, and one of them is a defect.

- **The working figure is ~322 MB, and the ceiling the file declares is 1.50 GiB.** The gap is
  headroom, which is defensible for a memory limiter and for a machine that also has a laptop's
  worth of other work on it. The brief's warning — "a stack that needs 16 GB of RAM to look at six
  services is not 'on by default' for anyone" — does not come close to describing this stack. The
  whole eight-container stack at rest is **274 MB**, which is less than a single `caf` build.
- **`postgres`, `nats` and `redis` have no memory bound at all.** The compose file's own comment
  says "A dev machine running six services plus four more needs every one of them capped, or the
  kernel kills the largest and the whole thing gets attributed to something else" — and then caps
  five of eight. `postgres` is the largest of the three by RSS in every reading I took. This is a
  one-line-per-service fix and it belongs with whatever converges on this template.

**Disk.** Volumes after the canary load: `grafana-data` 14.1 MB, `postgres-data` 45.9 MB,
`tempo-data` 300 KB, `loki-data` 64 KB, `mimir-data` 4 KB, `nats-data` 0, `redis-data` 12 KB —
about **60 MB total**, essentially empty.

> **I could not measure retention growth and I will not estimate it as if I had.** Twenty minutes
> of almost no telemetry does not produce a bytes-per-day figure, and the answer depends on
> ingestion rate and on the `compactor` and `retention_period` settings in `tempo/tempo.yaml`,
> `loki/loki-config.yaml` and `mimir/mimir.yaml`, which I read but did not exercise. **The volume
> figure above is a floor, not an estimate of a week's operation**, and whoever implements this
> should run the stack for a day at a realistic span count before quoting a disk number to a
> customer. See §11.

### 3.3 The failure mode that matters more than the footprint

`docker compose up` the template from a directory that does not contain the four vendor config
files, and this is what happens:

```
failed parsing config: failed to read configFile /etc/tempo/tempo.yaml: read /etc/tempo/tempo.yaml: is a directory
failed parsing config: read /etc/loki/loki-config.yaml: is a directory. …
error loading config from /etc/mimir/mimir.yaml: … read /etc/mimir/mimir.yaml: is a directory
Error: failed to get config: … unable to read the file file:/etc/otel/otel-collector.yml: read …: is a directory
```

Four of eight services exit(1) with a message that names a *directory*. Docker Compose creates a
missing bind-mount source as a directory, so a `cp` that missed `tempo/` produces a working-looking
mount of nothing. I confirmed the mechanism on a stack that was running on the machine when I
started: `docker inspect` shows the bind source exists on the host **and is a directory**.

**And here is the part that matters.** I built the Grafana block in isolation with its provisioning
mount pointing at a path that does not exist, and measured what a self-hoster is told:

| | result |
|---|---|
| container healthcheck | **`healthy`** |
| `GET /api/health` | `{"database":"ok","version":"11.3.0",…}` — nothing more |
| `GET /api/datasources` | **`[]`** |
| `GET /api/search?type=dash-db` | **`[]`** |
| the only trace | one `level=error` line: `can't read dashboard provisioning files from directory` |

**Grafana — the one service the owner's question is about — is the one service that comes up green
when its configuration is missing.** The three datasources it reads from are gone, both shipped
dashboards are gone, and the health endpoint says everything is fine. This is the single most
important measured result in this report after §5, because it means:

> **"On by default" cannot be implemented as "the compose file is present".** It has to be a check
> on the thing the user actually wanted — that Grafana has its datasources and the shipped
> dashboards exist, and that a query against each datasource returns — not on process liveness.
> Every liveness signal in this stack is satisfiable by a stack that contains nothing.

`kit`'s gate cannot catch this either, because `tests/validate.sh` reads the template's *text*; it
never brings the stack up. That is the gap to close first, and `scripts/redaction_canary.py` is
the shape of the answer: bring it up, ask the thing the user wants, fail if the answer is empty.

---

## 4. Answer 3 — "all errors in one place", which is the genuinely under-specified part

### 4.1 Decide what an "error" is here, defensibly

Traces, metrics and logs are not errors, and the brief is right. Here is the definition I
recommend, and it is already `PLAN.md`'s:

> **An error is a span whose status is `ERROR`.** Not a log line, not a metric above a threshold,
> not a string. The predicate is span status, and the dimension is `error.type` — core's thirteen-value
> vocabulary, loaded from `core/schemas/telemetry/traces.schema.json` rather than retyped.

Three properties make this defensible rather than convenient:

1. **It is a single query across every service, with no per-service instrumentation.** It is a
   property of the OpenTelemetry span model, so every service answers it the same way whether or
   not anyone thought about it. `muse` has already done the hard part — `muse.errortype` loads the
   enum out of core's vendored schema and **raises** on an unmapped class rather than defaulting,
   which is the correct failure direction and the reason the fleet-wide view is a query at all.
2. **It is already implemented and already drawn.** The `spanmetrics` connector projects span status
   onto a metric, `redaction/cafaye_metrics` re-derives the `status_code` label because the
   allowlist dropped it, and `cafaye-fleet-errors.json` has panels named *"Error ratio (fleet, last
   5m)"*, *"Error rate by service"* and *"Error class, inside one service"* that query exactly
   this. **The answer to the owner's third question is a dashboard that already ships and already
   has the right shape.** It has never been run against a real fleet, because no service emits.
3. **It cannot carry content, by construction.** `error.type` is a bounded snake_case token from a
   closed list. There is no free text in the predicate. This is the whole reason it is the right
   definition of an error *for this platform specifically*: an error aggregator's job is to make
   one bad request out of a million legible, and the legible unit here is the class, not the text.

**What "in one place" then means, stated exactly:** Mimir for the count and the rate (the
aggregate), Tempo for the one trace when someone has a trace id, and Grafana to join them —
`cafaye — every error in the fleet` already names all three. Adding a fourth store is only worth
it if one of those three cannot answer a question the other two cannot.

### 4.2 What the log/crash layer contributes, and what it costs

The second half of the fleet-wide error view is the crash layer: a panic in a language with no SDK,
delivered as a log record. That is a genuinely different signal from a span status, and it is the
only way to see an error from a process that died before it could report one.

**Measured: it currently carries nothing, and it is not supposed to.**

- `kit/templates/compose/otel-collector.yml:104-106` states: *"The compose template sets `logging:`
  on each service, which is where that is configured."*
- `kit/templates/compose/docker-compose.yml:210-212` states: *"it is the endpoint Docker's syslog
  log driver delivers to over the compose network, which is why every service below sets
  `logging:`."*
- **`grep -c 'logging:'` on the 433-line compose file returns 1, and that one occurrence is inside
  the comment on line 212.** No service declares a logging driver. No container's stderr is
  delivered to the `syslog/crash` receiver.
- **Measured at the other end:** I wrote two lines to a running container's stderr containing a
  `PANIC:` and a `Bearer CRASHBEARER-…` string, and Loki's only `service_name` value afterwards was
  the one service whose records had arrived over OTLP. Zero crash records.

So the crash layer is a receiver, a transform and a pipeline with **no producer**. Both documents
assert a thing that is false, in the file that is the template for a feature nobody has used — which
is how a false claim survives: there is no adopter to contradict it.

**And this is the same failure the template already documents one layer down.** The compose file
explains at length why Mimir's healthcheck must be `/bin/mimir` and not `/mimir`: "the data
directory is mounted at /mimir, so the bare path is a DIRECTORY and exec fails with 'is a
directory'". The same class of error, one layer up, is shipped, and the compose file's own comment
about it is the thing asserting the configuration that does not exist.

### 4.3 The error tracker evaluation, with every licence checked from source

Every licence below was read from the repository's own `LICENSE` file on **2026-09-30**, at the URL
printed. None is cited from a blog post, a comparison article, or a vendor's marketing.

#### 4.3.1 The incumbent: self-hosting Sentry — **no, and the numbers are not close**

**Licence: FSL-1.1-Apache-2.0 — and so is Sentry itself, not only the packaging.**

`https://raw.githubusercontent.com/getsentry/sentry/master/LICENSE.md` and
`https://raw.githubusercontent.com/getsentry/self-hosted/master/LICENSE.md`, both HTTP 200,
identical first line:

> `# Functional Source License, Version 1.1, Apache 2.0 Future License`

This is the part the brief predicted nobody would check, and it is worse than "the self-hosted
packaging is source-available": **`getsentry/sentry` — the server — is FSL-1.1 too.** The two
clauses that decide it, quoted:

> `### Permitted Purpose`
> A Permitted Purpose is any purpose other than a Competing Use. A Competing Use means making the
> Software available to others in a commercial product or service that: 1. substitutes for the
> Software; 2. substitutes for any other product or service we offer using the Software that exists
> as of the date we make the Software available; or 3. offers the same or substantially similar
> functionality as the Software.

> `## Grant of Future License`
> We hereby irrevocably grant you an additional license to use the Software under the Apache
> License, Version 2.0 that is effective on the second anniversary of the date we make the Software
> available.

**The honest reading, which I am not entitled to make.** FSL-1.1 is not an OSI-approved licence.
Shipping self-hosted Sentry *inside* a commercial product is very likely a Permitted Purpose —
cafaye would not be substituting for Sentry. It is also very likely that the "Competing Use" test is
one only a lawyer should apply, and the fleet has already been burned once on exactly this axis
(MD10: gitleaks is MIT, trufflehog is AGPL-3.0, and the whole tooling choice turned on it). **The
practical fact the owner needs is that the licence is a decision, not a default, and that pinning a
Sentry version older than two years converts that version to Apache-2.0** — which is a usable
mechanism, if the owner is willing to run a two-year-old error tracker.

**Footprint, measured from `getsentry/self-hosted`'s own `docker-compose.yml` (756 lines, 32,624
bytes, HTTP 200 at the URL above):**

- **53 services.** Counted at exactly two-space indent under `services:`. Not an estimate.
- **3 of its 16 image refs are built locally** — `sentry-self-hosted-local`,
  `clickhouse-self-hosted-local`, `sentry-cleanup-self-hosted-local` — with a `build:` stanza. A
  self-host is a source build.
- **8 more image refs come from `.env` in the same repository, and every one of them is
  `:nightly`** — `ghcr.io/getsentry/{sentry,snuba,relay,symbolicator,taskbroker,vroom,uptime-checker,launchpad}:nightly`.
  The stack's own pins are a moving tag.
- **`grep -c 'mem_limit' docker-compose.yml` returns 0.** The stack declares no memory bound for
  any of its 53 containers. There is no ceiling to quote.
- **Image weight, measured from the two registries.** amd64 compressed pull: the eight
  `getsentry` images are **1,500.8 MB** and the seven resolvable third-party images are **834.1 MB**
  — `confluentinc/cp-kafka:7.6.14` alone is 530.4 MB — for **2,334.9 MB** before the two
  locally-built images, which I did not build and therefore did not size. Layer sharing between
  the eight `getsentry` images is unknown to me and would reduce the true figure.

**Verdict: no.** Not because of the licence alone, and not because of footprint alone — because a
53-container, nightly-tagged, source-built stack that declares no memory limit is not a component a
customer can be told to run, and the brief's own standard is "one more thing that has to still work
in three years."

#### 4.3.2 The Sentry-API-compatible lighter alternative: GlitchTip

**Licence: MIT.** Read from the canonical source, not the GitHub mirror —
`https://gitlab.com/glitchtip/glitchtip/-/raw/master/LICENSE`, HTTP 200:

> `MIT License`
> `Copyright (c) 2023 David Burke`

(For the record, the GitHub repository `GlitchTip/GlitchTip` is a **stale mirror last pushed
2022-08-21 with no licence file at all**, and GitHub's SPDX detection for it is unreliable. Cite
the GitLab URL or cite nothing.)

**Footprint.** One image: `glitchtip/glitchtip` measures **706.3 MB unique on disk** on this
machine, against 1,334.1 MB for the entire eight-image LGTM stack. A Django application of that
shape needs Postgres and Redis beside it, so three containers, not fifty-three.

**What you get that the metrics-only view does not:** real grouping and dedup, stack traces,
release tagging, and a per-issue alert. **What it costs you:** it is a second store, and its
ingest path is *not* the OTel collector — so every error it receives has travelled a route that
does not pass through `redaction/cafaye_*`. That is the whole argument against it, and §5 says
what happens on a route that bypasses the collector.

#### 4.3.3 The other candidates, briefly and honestly

| candidate | licence | read at | what I found |
|---|---|---|---|
| **SigNoz** | MIT **outside `ee/` and `cmd/enterprise/`** | `https://raw.githubusercontent.com/SigNoz/signoz/main/LICENSE`, HTTP 200: *"Content outside of the above mentioned directories … is available under the 'MIT Expat' license"*, and `ee/` is licensed under a **separate, unreviewed-by-me** `ee/LICENSE` | An all-in-one OTel UI, which is the right shape. I did not measure it: the stack is ClickHouse + Postgres + Redis + Kafka/Zookeeper plus ~10 signoz services, and the two heaviest of those were not already on this machine. Not recommended and not quantified. |
| **Highlight** | Apache-2.0 **outside `highlight.io/` and `enterprise/`** | `https://raw.githubusercontent.com/highlight/highlight/main/LICENSE`, HTTP 200 | Two carve-out directories with their own licences, same shape as SigNoz. Not measured. |
| **Grafana + Loki + Tempo + Mimir** | **AGPL-3.0** — all four, verified below | see §7 | **The stack that is already in the distribution.** |

I did not build a bespoke error aggregator and I am not recommending one. The platform's rule is
that wheels get reused, and the wheel here is a hundred lines of Grafana JSON that is already
written, already provisioned, and already queried the way the answer should be queried. The only
thing it is missing is the canary test that proves its redaction holds — which is §5.

### 4.4 The case for building the error path out of what the platform already emits, argued properly

The honest version of this case is not "we don't need an error tracker". It is: **the error path
this platform has is already a redaction boundary, and swapping it for a tracker means swapping a
boundary I can test for a boundary I cannot.**

**What building it out gives:**

- **One store, three signals, one join.** A trace id on an error log record resolves to the trace.
  That correlation is the whole ergonomic win and it costs one field.
- **Grouping for free, because grouping is a query.** "Every error, by class, by service, last
  five minutes, ordered by rate" is
  `topk(15, sum by (error_type) (rate(cafaye_calls_total{service_name=~"$svc", status_code="STATUS_CODE_ERROR"}[5m])))`
  — and it is already a panel titled *"Error class, inside one service"*.
- **Zero new licences, zero new containers, zero new ingest routes**, and 281 MB of RAM already
  measured. The error question costs **nothing** to answer, which is the strongest argument for it.
- **A boundary that is already three independent controls** and which I measured working on the
  trace path (§5.3): an allowlist, `blocked_values`, and an explicit span-event delete.

**What it gives up against a real error tracker — named, not waved at:**

| | metrics/logs/Explore | a real tracker |
|---|---|---|
| **Grouping** | you aggregate by a class you chose. Two different exceptions that both map to `_OTHER` are one row. | content-based fingerprinting groups by the actual fault site |
| **Stack traces** | **not available at all** — `exception.stacktrace` is deleted by design, and correctly | the single most useful field when you have a bad deploy |
| **Release tagging** | `service.version` is on the resource and survives; a diff between releases is a Grafana query you write | a first-class release, a regression marker, a suspect-commit range |
| **Dedup** | `spanmetrics` counts occurrences; it does not dedup | one issue per fault, with an occurrence count |
| **Alerting** | the shipped `rules.yml` has fleet rules; you write your own | per-issue, with thresholds and ownership, and it pages you |
| **The text** | absent by design, everywhere, always | present, which is the whole point and the whole hazard |

**The killer row is "the text".** A tracker you cannot search by message is not a tracker; and a
tracker you *can* search by message is a searchable store of your customers' prompts, held by a
third party, indexed, retained, and readable by anyone with dashboard access. **Those two rows are
not tradeable against each other — they are the same row.** The only way to have both is to
enforce a redaction boundary on the tracker's ingest path, and the one thing I measured in this
report is that **such a boundary is exactly what the shipped pipeline does not currently do on its
error path** (§5.3). So the recommendation is not "don't add a tracker" — it is **"add a tracker
only after the boundary exists and is tested"**, and the boundary is the prerequisite, not the
tracker.

---

## 5. The redaction rule, stated so a component can be tested against it

The platform's settled rule is that **prompt and completion content must never appear in a
telemetry span**. `core/schemas/telemetry/redaction.schema.json` (read at the ref
`vendir.lock.yml` records, `71d01fd…`) makes it checkable, and the part I rely on is its own
description: *"The default is deny. A telemetry pipeline that drops an attribute it does not
recognise is the only shape of this control that survives a well-meaning change made six months from
now."* That is right, and the measurement below is what happens when the rule is applied to
attributes and **not** to the two other fields that are not attributes.

### 5.1 The rule, as numbered predicates

Each predicate is a thing a component can be asked and answer yes or no. `scripts/redaction_canary.py`
is the executable form of this list.

> **R1 — Attributes are default-deny.** A span, log record or metric datapoint carries **only** an
> attribute name on the signal's allowlist. An unrecognised name is **deleted**, not passed through.
> *Test:* plant `cafaye.prompt` on a span and a log record; both must be absent from the store.
>
> **R2 — A value-shape barrier runs independently of the name barrier.** A credential-shaped value
> is masked **in place** even when the attribute carrying it is allowlisted. Applies to JWTs,
> `sk-`-prefixed keys, and `Bearer <token>`.
> *Test:* plant `sk-live-…` under `http.route` and `llm.model`, both of which are allowlisted; both
> must read `****` in the store, and the underlying value must be unreachable.
>
> **R3 — `exception.message` and `exception.stacktrace` are deleted from span events**, always, and
> never re-added.
> *Test:* plant both on an exception span event; neither may appear in the trace.
>
> **R4 — `span.status.message` is a prohibited field, not merely an unlisted one, and is set to a
> bounded class or empty.** Status carries `STATUS_CODE_ERROR` and nothing else.
> *Test:* plant a canary in `status.message` on an error span; it must not appear in the store.
> **This predicate is not met today — see §5.3.**
>
> **R5 — A log record's `body` is a prohibited field.** The free text of a log record is scrubbed
> to a bounded class, or is not shipped. The body is the single most likely place for a credential,
> because it is where a third party's error message arrives.
> *Test:* plant a canary and a credential-shaped string in a log record's body; neither may appear
> in the log store. **This predicate is not met today — see §5.3.**
>
> **R6 — A type is refused, never a value.** A credential wrapper is refused on its *type* at the
> point of recording, not by comparing its value. A filter with a bypass is the thing to avoid.
> *Test:* attempt to record a `Secret` through the recording path; the call must fail by type.
> *This is `muse`'s existing, correct behaviour and the only implementation in the fleet.*
>
> **R7 — R1, R2 and R3 are the allowlist; R4 and R5 are the two fields the allowlist cannot
> express.** The allowlist governs *attributes*. It does not govern `Status.message`, it does not
> govern a log record's `body`, and it does not govern anything inside a serialized string. **Any
> component that implements R1–R3 and claims R4–R5 by implication is wrong, and the difference is
> measurable.**

### 5.2 Why it cannot be delegated to a linter

The fleet already knows this and the knowledge is in the right place. From MD10, measured across
the fleet's own tooling: gosec's `credentials.Match` switches on four AST node types with no
`*ast.CallExpr` case — it finds literals, not a token passed to a logger. Bandit matches
`ast.Constant` only. Brakeman's secret check is optional and off. **Of 268 Semgrep taint rules,
zero intersect CWE-532.**

A secret leaked *at runtime* is, by construction, a value in a data structure on the request path.
No static analyser can see it, because the whole event is a value being handed to a function at
run time. So R1–R7 must be enforced **at the boundary** — the one process every signal passes
through — and the enforcement has to be a test, not a review.

### 5.3 Enforcing it: what it cost, and what it found

`kit` chose the right enforcement point. `core/schemas/telemetry/redaction.schema.json` puts
`enforcedAt: collector` in the schema as a checked value, and the collector carries three
independent controls: `redaction/cafaye_{traces,logs,metrics}` with `allow_all_keys: false` (the
collector is fail-closed — an empty allowlist strips everything), a `blocked_values` list, and
`transform/cafaye_span_events` deleting `exception.message` and `exception.stacktrace`.

**I tested all of it against a real running stack.** `scripts/redaction_canary.py` sends 40 spans
and 40 log records, one per request, each carrying a distinct canary in eight positions, then reads
them back out of Tempo and Loki. Nothing in the script is a secret: the canaries are fixed greppable
strings, and `sk-live-CANARYKEY-…` and `Bearer CANARYBEARERTOKEN-…` are *shaped* like credentials so
the collector's regexes have something to match. No real endpoint, token, prompt or customer datum
appears in the script or its output.

**Result, traces — the boundary holds, and all three barriers fired.** Canaries found in Tempo:
**none** of `prompt_attr`, `exc_message`, `exc_stack`, `sk_under_allowed_key`,
`bearer_under_allowed_key`. The collector's own debug exporter shows the three receipts:

```
error.type=provider_unavailable http.route=/v1/route llm.model=claude-opus-4 http.response.status_code=503
llm.model=****  http.route=****  redaction.redacted.count=1  redaction.masked.count=2
```

One attribute deleted (R1), two values masked in place (R2), the exception attributes gone (R3). The
attributes that survive are exactly the allowlist. **This is a good piece of engineering and it
works.**

**Result, logs — the boundary does not hold on the error path.** Read back from Loki:

```
STREAM LABELS:  deployment_environment=canary  detected_level=error
                error_type=provider_unavailable  log_severity=ERROR
                redaction_ignored_count=2  redaction_redacted_count=1
                scope_name=canary  service_name=muse  severity_number=17
LINE: CANARYLOGBODY-0040 upstream rejected sk-live-CANARYKEY-AAAAAAAAAAAAAAAAAAAA
```

The `cafaye.prompt` attribute **was** dropped (`redaction_redacted_count=1`), and the two resource
keys were exempted (`redaction_ignored_count=2`) — the allowlist did its job on the record. **The
body went through untouched, carrying both a prompt-shaped string and a credential-shaped `sk-`
key.** R5 is not met.

**Result, span status — the boundary does not hold there either.** `status.message` is a field of
the OTel `Status` message, not an attribute, so no allowlist reaches it. A trace read back from
Tempo:

```json
"status": {"message": "CANARYSTATUS-0003", "code": "STATUS_CODE_ERROR"}
```

**16 of 16** canary traces carried the status message verbatim. R4 is not met. (My first run
reported this canary as absent; that was sampling luck over Tempo's partial search result, and
re-running the question per-trace rather than per-sample is what corrected it. The
`redaction_canary.py` left in this commit now asserts the floor — that `error.type`, `http.route`,
`llm.model` and `service.name` *did* arrive — before it reports any absence, precisely so an empty
readback can never be read as a pass. That discipline is `muse`'s own, from
`test_trace_propagation.py`, and I copied it because I got it wrong first.)

**One more, smaller: the redaction receipt leaks onto the wrong signals.** `redaction.redacted.count`,
`redaction.masked.count` and `redaction.ignored.count` are documented at
`otel-collector.yml:503-521` as being removed after the redaction step — and
`transform/cafaye_metrics_labels` does remove them, **on the metrics pipeline only**. They ride out
on traces and on log records, where they become index labels in Loki. Not a security problem, and a
cardinality axis nobody queries.

**So, what did enforcing it cost?** Three barriers in a config file, one `transform` processor, and
a `validate.sh` that derives the allowlist from core's schema and compares both ways. That is the
cheap part and it is already paid for. What is missing is **two more controls on two fields the
allowlist cannot express**, and one structural repair:

1. **A `transform` on the traces pipeline that overwrites `status.message` with a bounded value**
   (or empties it). Small, and the collector supports rewriting a status.
2. **A scrubber on the logs pipeline for the record body** — `transform/cafaye_logs` with
   `replace_pattern`, or a `redaction` processor configured to treat the body. This one is a real
   design question, because the body is unstructured and a regex scrubber is a scrubber, and the
   honest options are (a) regex-scrub the body for the shapes in `blocked_values` and accept that
   it is a denylist, or (b) **do not ship the body at all** — carry `log.severity`, `error.type`
   and the resource attributes, and let the body exist only in whatever the service's own
   destination receives. Option (b) is the boring one, it is what R5 says, and it costs the
   platform the free text of a log record, which for an *error aggregator* is an acceptable price
   and for a *log aggregator* is not — which is why this is a decision for the owner and not for me.
3. **A `logging:` block per service**, or the crash layer stays dead. §4.2.

### 5.4 The measurement that is uncomfortable: "accepted" is not "stored"

While measuring the above I sent a single OTLP/HTTP POST carrying **4000 spans** (3.77 MB) to the
shipped collector, twice, plus 572 log records and 140 metric points in the same shape. Every
request returned **HTTP 200** with a body of **`{"partialSuccess":{}}`** — an explicit
*no rejections*.

`otelcol_receiver_accepted_spans` did not move. `otelcol_processor_incoming_items` did not move.
`otelcol_exporter_sent_spans` did not move. Nothing appeared in Tempo, and Loki's `labels` endpoint
had no series for those records. A single-record request in the same shape was accepted, exported to
both exporters, and searchable in Tempo within 20 seconds.

**I do not know why, and I am not going to guess.** It is plausibly my hand-written OTLP/JSON — the
probe span used a shape I was confident in and the batches did not, which points at my client
rather than at the collector. It is also plausibly a receiver-side limit, and the two are not
distinguishable without instrumenting the receiver, which I did not do.

What is *not* in doubt, and is the part that matters to the contract in §2.3: **a request that the
collector answered `200 OK, nothing rejected` produced nothing observable at the far end, and the
only way I found out was by asking the store.** A self-hoster has no reason to ask the store. That
is the strongest argument I have for the boot check being a round trip, and for the canary test
being a gate rather than a document.

---

## 6. On-but-not-required, and what it costs in dev

### 6.1 The brief's own question: full stack, profile, or subset?

**The stack already answers this and the answer is good: `profiles: [observability]`.** Five of the
eight services carry it, so `docker compose up` on a constrained machine gives you
postgres + nats + redis + the collector, and `--profile observability` adds Tempo, Loki, Mimir and
Grafana. `bin/dev` passes the profile by default, so the default path is the whole thing. **That is
the right shape and it needs no redesign.** What it needs is the two repairs in §3.3 and §5.3,
because a profile that can be "on" while Grafana has no datasources is not on.

**The measured dev cost, from §3.2:** 281 MB of RAM resident for the observability five, 1,334 MB
of images pulled once, about 60 MB of volumes at rest. The one-time cost is the images; the
standing cost is a fifth of a gigabyte and eight processes. **On a laptop that is the right
trade for what it buys, and it is worth saying so out loud, because the answer to "is this
default-reasonable" is yes and nobody should have to run it to find out.**

### 6.2 The smallest thing that still catches the class of bug a developer hits locally

Not a smaller stack — a **different assertion**. The class of bug a developer hits locally is not
"a trace is missing". It is **"my new field never arrived, and I did not notice, and I spent an
afternoon on the wrong layer."** That bug is invisible in every signal at once, and the cheapest
thing that catches it is the one a developer can see in their own terminal with no infrastructure
running at all.

**Recommendation: the minimum useful dev posture is `set(attributes, "cafaye.dev", "1")` in
developer builds, printed to stderr by the SDK's console exporter.** No collector, no profile, no
container. It costs one conditional and it makes span emission *visible in the process that emits
it*. A developer who cannot see the span cannot debug the span.

**And the floor is the canary, not the dashboard.** The one observability test a developer must be
able to run without Docker is the redaction one, because a leak in their code is worse than a
missing span. `scripts/redaction_canary.py` needs a running stack; the same eight canaries can be
asserted against an `InMemorySpanExporter` with no infrastructure at all, which is exactly what
`muse/tests/test_trace_propagation.py` already does and what I would require of every service
before it is allowed to emit.

### 6.3 How does a developer *know* it is running?

Today: **they do not, and the answer is a file, not a flag.** Three signals, cheapest first:

1. **One line at `bin/dev up`**, naming the profile and the URL, e.g.
   `observability on — grafana http://localhost:15000, 5 services, ~281 MB`. A developer who is not
   told will not look, and a developer who has to guess will assume it is off.
2. **`/readyz` carries a `checks` map that is already the right shape.** `muse`'s `AGENTS.md` rule 6
   says the `db` slot is `ok`, or `error` with status `degraded`. Adding an `otel` slot that reads
   `ok` / `off` / `unreachable` costs nothing, needs no dependency, and puts the answer in the one
   endpoint a developer and a monitor both already poll. **It must not fail readiness** — the
   `degraded` precedent is exactly right, and the existing rule that telemetry is never in a
   readiness path still holds: reporting *is* not *gating*.
3. **The canary in CI, not a dashboard in the browser.** A developer believes a check that is red.
   Nothing a developer looks at is ever red.

---

## 7. Licence table — every third-party component, quoted from source

All read **2026-09-30** from the `LICENSE` file at the URL printed, `raw.githubusercontent.com`
unless stated. None cited from a secondary source.

| component | version pinned by kit | licence | the quoted line | read at |
|---|---|---|---|---|
| `postgres` | `16.6-alpine` | PostgreSQL Licence | *not checked — see §11* | — |
| `nats` | `2.10.24-alpine` | Apache-2.0 | *not checked — see §11* | — |
| `redis` | `7.4.1-alpine` | *not checked — see §11* | — | — |
| `otel/opentelemetry-collector-contrib` | `0.115.1` | **Apache-2.0** | `Apache License` / `Version 2.0, January 2004` | `https://raw.githubusercontent.com/open-telemetry/opentelemetry-collector-contrib/main/LICENSE` |
| `otel/opentelemetry-collector` (core) | — | **Apache-2.0** | `Apache License` / `Version 2.0, January 2004` | `https://raw.githubusercontent.com/open-telemetry/opentelemetry-collector/main/LICENSE` |
| `grafana/grafana` | `11.3.0` | **AGPL-3.0** | `GNU AFFERO GENERAL PUBLIC LICENSE` / `Version 3, 19 November 2007` | `https://raw.githubusercontent.com/grafana/grafana/main/LICENSE` |
| `grafana/tempo` | `2.7.2` | **AGPL-3.0** | `GNU AFFERO GENERAL PUBLIC LICENSE` / `Version 3, 19 November 2007` | `https://raw.githubusercontent.com/grafana/tempo/main/LICENSE` |
| `grafana/loki` | `3.2.1` | **AGPL-3.0** | `GNU AFFERO GENERAL PUBLIC LICENSE` / `Version 3, 19 November 2007` | `https://raw.githubusercontent.com/grafana/loki/main/LICENSE` |
| `grafana/mimir` | `2.13.0` | **AGPL-3.0** | `GNU AFFERO GENERAL PUBLIC LICENSE` / `Version 3, 19 November 2007` | `https://raw.githubusercontent.com/grafana/mimir/main/LICENSE` |

**Candidates evaluated and not recommended:**

| candidate | licence | the quoted line | read at |
|---|---|---|---|
| **Sentry (server)** | **FSL-1.1-Apache-2.0** | `# Functional Source License, Version 1.1, Apache 2.0 Future License` | `https://raw.githubusercontent.com/getsentry/sentry/master/LICENSE.md` |
| **Sentry self-hosted** | **FSL-1.1-Apache-2.0** | `# Functional Source License, Version 1.1, Apache 2.0 Future License` | `https://raw.githubusercontent.com/getsentry/self-hosted/master/LICENSE.md` |
| **GlitchTip** | **MIT** | `MIT License` / `Copyright (c) 2023 David Burke` | `https://gitlab.com/glitchtip/glitchtip/-/raw/master/LICENSE` |
| **SigNoz** | MIT outside `ee/` and `cmd/enterprise/` | *"Content outside of the above mentioned directories or restrictions above is available under the 'MIT Expat' license"* | `https://raw.githubusercontent.com/SigNoz/signoz/main/LICENSE` |
| **Highlight** | Apache-2.0 outside `highlight.io/` and `enterprise/` | *"Content outside of the above mentioned directories or restrictions above is available under the 'Apache' license"* | `https://raw.githubusercontent.com/highlight/highlight/main/LICENSE` |

**The AGPL-3.0 argument, which kit already makes and which I verified.** The four backends are
AGPL-3.0 and are shipped **unmodified**: `image:` is a stock `grafana/<name>:<pinned tag>`, no
`build:` stanza on any of the four (I checked — the only `build:`-shaped line in the template is a
comment), the configuration files under `loki/`, `tempo/`, `mimir/` and `grafana/` are mounted
read-only, and nothing is overridden onto the vendor's own image contents. AGPL §13's obligation
runs on the Grafana *server*; the applications it observes are not derivative works of it. **A
self-hoster using cafaye is not thereby offered a modified Grafana.** That is the correct reading
and it is the same shape as the gitleaks/trufflehog ruling in MD10: a permissive licence for the
thing we depend on, a copyleft licence for a thing we run unmodified, and the distinction is a
fact about the deployment, which is why `kit`'s gate asserts "no `build:` stanza on the AGPL four"
rather than asking the reader to trust a README.

I checked that condition rather than repeating it: `grep -n 'build:'` on the 433-line template
returns **2**, and **both are inside comments** — line 267 is the statement of the rule, line 382 is
the Mimir healthcheck note that says adding a shell "is a `build:` stanza". No service carries one.
The condition holds.

---

## 8. Recommendations, one per section, each with a runner-up and its winning condition

### 8.1 §2 — the on-by-default / bring-your-own contract

**Recommended.** One variable per service, `<SERVICE>_OTEL_ENDPOINT`, defaulting to the shipped
collector; overriding it is a deployment decision that stops the platform's own stack, not a
setting that silences one service; override unreachable ⇒ **fail at boot, once, with a bounded
timeout**; after boot every export failure is silent; telemetry is never in a readiness path
(already true, keep it true); `/readyz` gains a non-gating `otel` slot so a developer can see it.

**Runner-up: degrade quietly** — log once at boot, keep serving, drop telemetry. **It wins if and
only if** the operator has more than one service in the fleet, because the boundary in §2.3
("configuration errors loud, runtime errors quiet") is only implementable where there is one
place the operator is watching. With six services, one bad variable and five good ones is a state
where failing the one takes down a working fleet for a diagnostic problem — and in exchange the
operator loses exactly the signal they were trying to get. **If we ship more than one service
behind one operator, the runner-up wins and I would change my recommendation.**

### 8.2 §3 — Grafana and the footprint

**Recommended.** Keep `kit/templates/compose/`, converge the six thin compose files toward it as an
**include-able overlay** rather than a whole-file replacement, and repair four things before anyone
is asked to adopt it: a `logging:` block per service, `mem_limit` on `postgres`/`nats`/`redis`, a
`validate.sh` check that brings the stack up and asserts Grafana has three datasources and two
dashboards, and the §5.3 controls. **322 MB resident and 1.30 GiB of images is not a footprint
problem and should be quoted as the answer to "what does this look like when it breaks".**

**Runner-up: treat `kit/templates/compose/` as a dead snapshot and write a new, smaller
observability profile** (collector + Grafana only, with traces in memory and metrics dropped).
**It wins if** a customer's floor is a 1 GB VM, where eight containers and a 1.5 GiB declared
ceiling is a bad fit — and I would accept that trade, because a stack that does not fit is a stack
nobody runs, and the traces-only answer still delivers "on by default" for a developer.

### 8.3 §4 — all errors in one place

**Recommended.** **No error tracker.** Define an error as span status `ERROR` with `error.type` as
the dimension; answer "in one place" with the dashboard that already ships and already has the
right panels; add the crash layer once there is a producer for it; and **fix R4 and R5 before
anything else in this packet ships**, because the error path is where a credential ends up and the
measurement says the shipped boundary does not hold there. The whole answer costs **0 new images,
0 new licences and 0 new containers** beyond what is already in the distribution.

**Runner-up: GlitchTip** (MIT, one image, 706 MB, Sentry-API-compatible so a customer can migrate
to or from Sentry without an SDK change). **It wins if and only if** one of these becomes true:
(a) someone needs **stack traces** — they are deleted by R3 and there is no argument that would make
keeping them safe on a boundary this weak; (b) release-tagging and suspect-commit ranges become a
requirement rather than a nice-to-have; (c) the platform gets past about five services and the
per-issue alert model is worth more than the query. **On (a) alone I would switch**, and that is
the honest condition: my recommendation is only right while the text stays deleted, and R3 is a
rule I am endorsing.

---

## 9. Should the nine compose files converge back toward kit's, or is kit's a dead snapshot?

**The manager asked for a recommendation, so here it is: converge — but replace the artifact, do
not adopt the one that is there.**

The reasoning, in the order that decided it:

1. **Zero adoption is not a taste problem.** The evidence in the diff is that the template is
   *broken in a way nobody can see*. Grafana comes up `healthy` with zero datasources and zero
   dashboards; the collector, Tempo, Loki and Mimir exit(1) with a message naming a directory; the
   crash layer has no producer. A developer who tried this stack once, saw a healthy Grafana with
   no dashboards, and walked away has told nobody. **You cannot debug what you have not noticed
   failing**, and that is the most plausible reason for 0-of-14, not disagreement with the idea.
2. **But the six thin files are not wrong, and replacing them would be a downgrade.** 44–86 lines,
   an app and one database, one thing to read. `docker compose up` on those works. Making a service
   adopt a 433-line stack to *also* get observability is a worse trade than a profile they can
   ignore.
3. **So the shape is a split, not a merge.** The six thin files stay as `caf dev`'s default. The
   observability stack becomes a compose **overlay** a service includes, with the four vendor config
   files **vendored into the service** rather than bind-mounted from a sibling path — because the
   bind mount is what turns a missing file into a directory, which is the whole failure.
4. **And `kit`'s current `templates/compose/` is a dead snapshot that is also wrong.** It is not
   merely unused: it is unsafe in the specific way the brief asked about. Keep the idea, the
   `.env.example`, the redaction derivation and the dashboards — those are good and they are the
   asset. Replace the compose file and fix the collector.

**Cost of being wrong in this direction:** a service that includes an overlay and does not want
observability has one more file. **Cost of being wrong the other way** — treating it as dead and
writing a new stack — is that we throw away a correct redaction design, a correct dashboard, and
the measured footprint, and rebuild all three against a schedule. That is the expensive error.

---

## 10. The gate

`mise x -- ./bin/prime`, run under `bash` with `set -o pipefail`, exit code captured with
`${PIPESTATUS[0]}`. **This commit is a markdown report and one script, so the suite should not
move. It did not move. That is the finding below.**

| configuration | passed | failed | ignored | cargo-reported skips | `GATE_EXIT` |
|---|---|---|---|---|---|
| full workspace (`PANTRY_CAFAYE_ROOT` set) | **114** | 0 | 0 | 0 | **0** |
| pantry-only clone at `92ca41e` | **114** | 0 | 0 | **0** | **0** |

Thirteen test binaries plus doc-tests, 14 `test result:` lines. Passing/failing/ignored match the
addendum's figures exactly.

**The skip number, stated in the configuration it exists in — and a correction to how it is
visible.** A pantry-only clone skips **21** tests. I confirmed 21 by running
`cargo test --no-fail-fast -- --nocapture` and counting the `SKIP` lines: 8 in `tests/drift.rs`,
4 in `tests/recorded_copy.rs`, 3 in `tests/schema.rs`, 6 in `tests/core_pin.rs`. Each names the
variable to set and says *"A drift test that cannot see the real services has proven nothing, so
this is a skip and not a pass."*

**And here is the thing worth writing down.** `cargo test` reports **0 skipped in both
configurations**, and the `SKIP` text goes to stderr, which the test harness captures and discards
for a passing test. The 21 skips are visible **only** under `--nocapture`. So the two
configurations print the *same summary line* — `114 passed; 0 failed; 0 ignored` — and the only
difference is visible to a reader who already knows to look for it.

> A green badge from a pantry-only clone does not distinguish "the registry was checked against the
> real services" from "the registry was checked against itself". The addendum's 21 is right; what
> the number does not yet say is that **nothing in the default output makes the difference
> visible.** The first thing this packet would change, if it changed anything, is that: either
> `bin/prime` passes `--nocapture` for the three drift binaries, or the skip count is printed by
> `bin/prime` itself and a skip in the full-workspace configuration is a failure.

I did not change that, because this packet is a research packet and the brief scoped it to a report
and a measurement script. It is named here as the first implementation task, and it is the one
place where a small change buys a real guarantee.

---

## 11. What I could not verify

Nothing in this section is a hedge. Each item is something I did not check against real source or
an actual run, and each would change a number or a claim if it turned out otherwise.

1. **Retention-driven disk growth.** I measured ~60 MB of volumes after ~20 minutes of
   near-silence. That is a floor, not a projection. I did not run the stack at a realistic span
   rate for a day, so **I have no bytes-per-day figure and did not invent one.** The answer depends
   on `compactor` and `retention_period` in `tempo/tempo.yaml`, `loki/loki-config.yaml` and
   `mimir/mimir.yaml`, which I read and did not exercise. **Quote the image and RAM numbers; do not
   quote a disk-usage claim from this report.**
2. **Why the collector dropped 4000-span batches** (§5.4). I proved *that* it drops them and that
   the reply says nothing; I did not determine *why*. My hand-written OTLP/JSON is a live suspect
   and the receiver is another. Until this is resolved, **the 4000-span and 572-log and
   140-metric-point results in §5.4 should be read as "these did not arrive", not as "the collector
   cannot accept a batch".** A real SDK's batch is a different request from mine.
3. **RAM under load.** 281 MiB resident for the observability five is measured at rest with 40
   spans and 40 log records in the stores. A real fleet's collector RSS is dominated by batch
   size and cardinality, and the memory limiter is configured at 75% of a 384 MiB cgroup. **The
   steady-state figure for a busy collector could be several times this.** The 1.50 GiB declared
   ceiling is the number to design against, not the 281 MB.
4. **`postgres`, `nats` and `redis` licences.** I checked the seven components the report's
   conclusion rests on (the collector, the four AGPL backends, Sentry, GlitchTip, SigNoz,
   Highlight) and did **not** check the three data services. They are outside the observability
   question and I ran out of reason to spend the check on them. If the report's licence table is
   to be the fleet's licence table, those three rows are still open.
5. **SigNoz and Highlight footprints.** Neither was measured — no images on this machine, and I
   chose not to pull and run two stacks I was not recommending. Their licence carve-outs are
   recorded and unexamined (`ee/LICENSE`, `enterprise/LICENSE`, `highlight.io/LICENSE`).
6. **Sentry and GlitchTip RAM.** Every documentation URL I tried for both —
   `develop.sentry.dev/self-hosting{,/local,/installation}/`,
   `glitchtip.com/docs/self-hosting{,/docker/}` — returned **HTTP 404** on 2026-09-30. So I have
   **no vendor RAM figure and did not cite one.** The 53-service count and the 2,334.9 MB of
   images are measured from their own repository and registries; the memory requirement is not
   something I can state.
7. **GlitchTip's service count.** I did not find a compose file in `gitlab.com/glitchtip/glitchtip`
   and its GitHub mirror is a stale 2022 fork, so "three containers" is an inference from the
   image being a Django application, not a measurement. The **706.3 MB image size is measured.**
8. **Whether the collector's `blocked_values` can reach a log record's body at all.** I measured
   that it did not mask one. I did not read the collector's redaction processor documentation or
   source to learn whether that is a documented limitation or a misconfiguration, so §5.3's
   item 2 is framed as a decision with two options rather than as a one-line config fix. **It may
   be neither that easy nor that hard.**
9. **The full-workspace gate was run once.** 114/0/0, `GATE_EXIT=0`, on this machine with this
   workspace. I did not run it in CI, and a sibling repository merging mid-run would have shown
   up as drift, not as a flake — which is the point of `recordedAt`, but it is a single observation.
10. **No fleet service was modified, deployed, or run under load.** Every claim about *what a
    service emits* comes from reading its dependency manifests and source. Only `muse` has been
    observed to emit anything, and only via its own test suite, not by me running it.
11. **The `muse` / `<SERVICE>_OTEL_ENDPOINT` divergence is read from source, not observed.** I
    confirmed `muse/src/muse/main.py:162` reads `MUSE_OTEL_EXPORTER_OTLP_ENDPOINT` and that no
    service reads the template's spelling. I did not run `muse` with each variable set to prove
    which one wins, and I did not check whether `settings.yaml`/`routes.yaml` carries the other
    spelling as a fallback.
12. **`glitchtip/glitchtip:latest` was already on this machine** (706.3 MB), so I did not pull it
    and cannot confirm the digest behind that tag. I did not run it.

---

## 12. What I committed, and the order I would dispatch this in

- `REPORT-pantry-07-observability.md` — this file.
- `scripts/redaction_canary.py` — the executable form of §5's rule. Demonstrated end to end against
  a running stack; **it exits 1 on the current code**, which is the point.

**The order matters, and it is not the order the three questions were asked in:**

1. **Fix R4 and R5, and the `logging:` block** (§4.2, §5.3). Nothing else in this packet is safe to
   dispatch first, because these three are the difference between an error view that is safe to
   hand a customer and one that is not. The crash layer is a feature nobody has used, so fixing it
   is not a regression.
2. **Make `bin/prime` report the 21 skips** (§10). Cheap, and it is the only change here that turns
   a claim into a guarantee.
3. **Add a kit check that brings the stack up and asserts Grafana has 3 datasources and 2
   dashboards** (§3.3). This is the "on by default" contract made testable.
4. **Split `templates/compose/` into the data profile and the observability overlay, vendored
   config files, `mem_limit` on the three uncapped services** (§9). Then converge the six compose
   files onto the overlay.
5. **Rename `MUSE_OTEL_EXPORTER_OTLP_ENDPOINT` to `MUSE_OTEL_ENDPOINT`** and make `kit`'s check
   real (§1.3), so the bring-your-own contract has two implementations rather than one guess.
6. **Add `/readyz`'s non-gating `otel` slot** (§6.3) and the one-line `bin/dev` notice.
7. **Only then** decide about an error tracker — which, on this measurement, means doing nothing,
   unless someone needs stack traces.

*Nothing in this report should be read as a claim I could not measure. Where I could not measure
it, it is in §11, and it is there because a confident wrong number is worse than an honest gap.*
