# REPORT — `pantry-account-scope-01`

**What this packet delivered.** `account_scope.yml` for `pantry`, four rows, every
one `accountScoped: false`, and — the part the brief asked for before any of it —
an answer to the question the packet was really about: **core's checker reads
zero of `pantry`'s routes, and the reason is one token in a generated file.**

**What it also delivered, which the brief did not ask for and which is the part
that makes the declaration worth having:** `internal/httpapi/scope_declaration_test.go`,
six tests that enforce what the Python harness structurally cannot on this
repository. A declaration nothing checks is a copy nobody checks, and this one
would otherwise have been exactly that.

---

## 1. The answer, stated before the work

> Can core's checker see a route surface nobody wrote by hand, or is `pantry` the
> second service whose `route_defs` reads 0 while it actually has fourteen?

**It cannot, and `pantry` is the second service whose route count reads 0 — for
four routes, not fourteen.** The count is zero in the strict sense: core's own
`discover()` finds nothing, so `undeclared` cannot fire, `surface.minimum` cannot
be set above 0, `declaration-contradicts-code` cannot check a single row, and one
correctly declared row is reported `stale` **falsely**.

**And the OpenAPI document is not the answer either — but not for the reason the
brief offers.** The document is a complete, already-regenerated enumeration, which
is strictly better than a hand-kept list, and it declares `security: []` at
`openapi/v1.yaml:82`, which is `pantry`'s whole security posture in one line. So
"drive the enumeration from the spec" is **right**. What is wrong is the
substitution it implies: the spec carries **authentication**, not **row
ownership**. `security: []` answers "is there a credential"; the question this
format asks is "whose rows can this reach". Those are different questions, and no
single artifact answers both.

So the delivered answer is a **split, and the split is the finding**:

| half of the contract | where it comes from | why |
|---|---|---|
| **the enumeration** — which entry points exist | DERIVED: `openapi/v1.yaml` → `internal/api/api.gen.go` → the live router | it is already generated. A second hand-kept list is strictly worse than a copy of a generator's output. |
| **the verdicts** — which of them reach another publisher's rows | DECLARED: `account_scope.yml` | the document has nowhere to put them. `security: []` is not an answer about `publisher_id`. |

`internal/httpapi/scope_declaration_test.go` is what holds the two together, and
that is the honest answer to "a second file kept in step by hand": **a second
file is fine if something checks it.**

---

## 2. What `pantry` actually is, before the checker is asked anything

**Four operations, four `GET`s, no write verb.** `internal/api/api.gen.go` mounts
them and nothing else mounts them — and the grep that says so needs its output
read, not its count:

```
$ grep -rn "\.Get(\|\.Post(\|\.Put(\|\.Delete(" internal/ | grep -v _test.go
internal/httpapi/httpapi.go:284:	svc, err := s.catalog.Get(r.Context(), name)
internal/httpapi/problem.go:43:	if w.Header().Get("Content-Type") == "" {
internal/httpapi/problem.go:125:		id := r.Header.Get("X-Trace-Id")
internal/api/api.gen.go:707:		r.Get(options.BaseURL+"/v1/services", wrapper.ListServices)
internal/api/api.gen.go:710:		r.Get(options.BaseURL+"/v1/services/{name}", wrapper.GetService)
internal/api/api.gen.go:713:		r.Get(options.BaseURL+"/healthz", wrapper.Healthz)
internal/api/api.gen.go:716:		r.Get(options.BaseURL+"/readyz", wrapper.Readyz)
```

**Seven hits, four routes.** Three of the seven are `catalog.Get`, `Header.Get`
and `Header.Get` — a repository read and two header reads. **A grep cannot tell a
registration from a method call**, which is the first half of D18 and the reason
the count was never the answer. (`internal/api/api.gen.go` contributes exactly
the four; `internal/httpapi/httpapi.go:145` builds the router and mounts nothing.)

**No queue consumer, no cron entry, no CLI entry point.** `surface.kind: http`
and no `kind: job` rows. Courier declared three Oban workers precisely because a
route grep cannot see one; `pantry` has the same *shape* of question and the
opposite answer:

```
$ grep -rniE "oban|sidekiq|queue|consumer|cron|schedule" internal/ cmd/ --include='*.go' | grep -v _test.go
(no hits)
```

**No route resolves a publisher, and that is a measurement rather than an
omission.** The resolver exists — `pantry.current_publisher_id()` at
`migrations/00005_functions.sql:62`, and `pantry.begin_publisher(p_publisher_id
uuid)` at `:108` — and **neither is called from any Go file**. The service takes
the `pantry_public` role per connection (`internal/pantrydb/db.go:125`) and reads
`current_user` back (`:129`), and `openapi/v1.yaml:82` says `security: []`. With
no identity set, `current_publisher_id()` is NULL and `pantry.service_is_visible`
reduces to "published, or first-party in any state".

So all four rows are `accountScoped: false`, and the declaration says precisely
what that means: **"resolves no publisher"**, not "reaches no publisher's data".
Every one of them reads rows that carry a `publisher_id`. This is the same
distinction courier's report drew for its Oban rows, and it is why the closed
vocabulary's `public` is the honest member rather than `none`.

**The write path is absent by migration, not by omission**, and no row claims
one. `migrations/00008_publisher_identity_immutable.sql` is `with check (false)`:
a publisher and the table owner may not `UPDATE` `publishers` at all. Nothing in
`internal/` writes `publishers` — `internal/pantrydb/queries/catalog.sql` is four
reads.

---

## 3. The measurement, with the commands

### 3.1 What the checker says today, before the declaration

```
$ cd cafaye/core && python3 harness/account_scope_check.py <HEAD, archived to a scratch dir>
FAIL account-scope.declaration-missing: account_scope.yml is not in …/head-tree
    fix: write account_scope.yml; the format is documented in docs/account-scope.md and the schema is schemas/account-scope.schema.json

FAIL …/head-tree: 0 account-scoped, 0 not-account-scoped, 0 stale, 0 undeclared — 1 failure(s), 0 warning(s), so nothing was left unproven silently
(exit 1)
```

### 3.2 What the brief's two greps say, on `HEAD` so the new files are not counted

```
$ git grep -n "r\.Get(\|r\.Post(\|r\.Put(\|r\.Delete(" HEAD -- internal/
HEAD:internal/api/api.gen.go:707:		r.Get(options.BaseURL+"/v1/services", wrapper.ListServices)
HEAD:internal/api/api.gen.go:710:		r.Get(options.BaseURL+"/v1/services/{name}", wrapper.GetService)
HEAD:internal/api/api.gen.go:713:		r.Get(options.BaseURL+"/healthz", wrapper.Healthz)
HEAD:internal/api/api.gen.go:716:		r.Get(options.BaseURL+"/readyz", wrapper.Readyz)
HEAD:internal/httpapi/problem.go:125:		id := r.Header.Get("X-Trace-Id")
--- count ---
       5

$ git grep -n "Method\|Pattern\|operationId" HEAD -- internal/api/api.gen.go | wc -l
       2
```

Five hits for four routes, because one of the five is `r.Header.Get`. **A grep
cannot tell a registration from a method call**, which is the first half of D18
and is why the count is not the answer. The second grep's `2` is the whole of
what a marker grep sees in `api.gen.go`: `Method` and `Pattern` appear in type
names, and no operation is named in that file at all.

### 3.3 What the CHECKER sees, which is nothing

```
$ cd cafaye/core && python3 - <<'PY'
import sys
from pathlib import Path
sys.path.insert(0, "harness")
import account_scope_check as asc
repo = Path("REPO")
for src in ("internal/api/api.gen.go", "internal/httpapi/httpapi.go", "internal"):
    found, _, _ = asc.discover(repo, [src])
    print("%-34s -> %d registration(s)" % (src, len(found)))
PY
internal/api/api.gen.go            -> 0 registration(s)
internal/httpapi/httpapi.go        -> 0 registration(s)
internal                           -> 0 registration(s)
```

Zero, on the file that registers everything, on the file that builds the router,
and on a walk of the whole package tree. Not "disagreed about" — **nothing**.

### 3.4 Why, isolated to one token

```
$ cd cafaye/core && python3 - <<'PY'
import sys; sys.path.insert(0, "harness")
from account_scope_check import registrations
for line in ['\tr.Get(options.BaseURL+"/v1/services", wrapper.ListServices)',
             '\tr.Get("/v1/services", wrapper.ListServices)',
             '\tmux.HandleFunc("GET /v1/services", h)']:
    print("  %r\n    -> %s" % (line, registrations(line, "internal/api/api.gen.go")))
PY
  '\tr.Get(options.BaseURL+"/v1/services", wrapper.ListServices)'
    -> []
  '\tr.Get("/v1/services", wrapper.ListServices)'
    -> [('GET', '/v1/services', 'chi')]
  '\tmux.HandleFunc("GET /v1/services", h)'
    -> [('GET', '/v1/services', 'chi-1.22')]
```

`RECOGNISERS[0].pattern` requires a `/`-prefixed string literal as the **first**
term after `(`. Concatenation is handled, and the flag that says so is called
`concatenates` — but it reads the terms *after* the first literal, because
identity's own admin router writes `r.Get("/v1/accounts/{accountID}/"+adminPrefix+
"/audit-log", …)`. `oapi-codegen` emits the other direction. Both lines register
the same route on the same router, and the checker sees one of them.

**PLANT A — delete the token from four generated lines and nothing else changes.**

```
$ diff <unplanted>/internal/api/api.gen.go <planted>/internal/api/api.gen.go
707c707
< 		r.Get(options.BaseURL+"/v1/services", wrapper.ListServices)
---
> 		r.Get("/v1/services", wrapper.ListServices)
710c710
< 		r.Get(options.BaseURL+"/v1/services/{name}", wrapper.GetService)
---
> 		r.Get("/v1/services/{name}", wrapper.GetService)
713c713
< 		r.Get(options.BaseURL+"/healthz", wrapper.Healthz)
---
> 		r.Get("/healthz", wrapper.Healthz)
716c716
< 		r.Get(options.BaseURL+"/readyz", wrapper.Readyz)
---
> 		r.Get("/readyz", wrapper.Readyz)

$ python3 harness/account_scope_check.py <plantA>
WARN account-scope.nothing-scoped: all 4 declared entry point(s) are accountScoped: false

  not-account-scoped GET /v1/services  (internal/api/api.gen.go:707)
  not-account-scoped GET /v1/services/{name}  (internal/api/api.gen.go:710)
  not-account-scoped GET /healthz  (internal/api/api.gen.go:713)
  not-account-scoped GET /readyz  (internal/api/api.gen.go:716)

OK …/plantA: 0 account-scoped, 4 not-account-scoped, 0 stale, 0 undeclared — 0 failure(s), 1 warning(s); warnings do not move the exit code
(exit 0)
```

Same four rows, same declaration, same four `registeredAt` line numbers. What
changed: **1 failure and 5 warnings became 0 and 1**, and the four
`contradiction-unreadable` warnings — "this checker could not read the code behind
it" — became four machine-verified verdicts. The checker then read each handler,
looked for account evidence, and found none, which is a **verdict** rather than a
gap.

That difference cannot be applied to this repository: `api.gen.go` is
`go:generate`d (`internal/api/generate.go:26`) and the token comes back on every
regeneration.

### 3.5 The floor cannot be the ratchet here

`schemas/account-scope.schema.json:68` calls `surface.minimum` "the
counterweight to what this file cannot do … a route in one of those four shapes
written a fifth way is INVISIBLE to it … this floor is what notices the second
kind". On `pantry` it counters nothing, because it counts what `discover` found:

```
$ grep -n "minimum" <copy>/account_scope.yml
8:  minimum: 4
$ python3 harness/account_scope_check.py <copy with minimum: 4>
FAIL account-scope.surface-thin: the declared sources register 0 entry point(s) and surface.minimum is 4
FAIL account-scope.stale: get-service: registeredAt points at a line that does not register it: internal/api/api.gen.go:710
(exit 1)
```

`minimum: 4` is **permanently red on a tree that serves four routes.** The
declaration pins it at 0 and says why in a comment, and the ratchet's job is done
by the Go test instead (§5).

### 3.6 The one FAIL is false about the code, and it is the most honest row

```
$ cd cafaye/core && python3 harness/account_scope_check.py <this checkout>
WARN account-scope.contradiction-unreadable: list-services: declared accountScoped: false and this checker could not read the code behind it — no registration on the declared surface matched this row
WARN account-scope.contradiction-unreadable: get-service: … (same)
WARN account-scope.contradiction-unreadable: healthz: … (same)
WARN account-scope.contradiction-unreadable: readyz: … (same)
FAIL account-scope.stale: get-service: registeredAt points at a line that does not register it: internal/api/api.gen.go:710
    fix: update `registeredAt` to the line the route is registered on, or delete the row if the entry point is gone — line numbers do not participate in identity, so a refactor that moved it three lines down is the only thing this is for
WARN account-scope.nothing-scoped: all 4 declared entry point(s) are accountScoped: false

  not-account-scoped GET /v1/services  (internal/api/api.gen.go:707)
  not-account-scoped GET /healthz  (internal/api/api.gen.go:713)
  not-account-scoped GET /readyz  (internal/api/api.gen.go:716)
  stale              GET /v1/services/{name}  (internal/api/api.gen.go:710)

FAIL <this checkout>: 0 account-scoped, 3 not-account-scoped, 1 stale, 0 undeclared — 1 failure(s), 5 warning(s); warnings do not move the exit code
(exit 1)
```

`internal/api/api.gen.go:710` **does** register the route. The message is false
about the code and true about the checker, and here is the mechanism:
`_mentions_on` asks the recognisers first and, when they find nothing, falls back
to `canonical(path).split("/")[-1] in text`. For `/v1/services/{name}`, the last
segment after parameter-collapsing is the two-character string `{}`, which
appears in no Go file.

**The three rows that stay green do so by accident.** `services`, `healthz` and
`readyz` are each a substring of their own registration line, so the fallback
happens to land. The row whose path ends in a parameter — the one spelled exactly
as `openapi/v1.yaml:222` spells it — is the only one that goes red. **A check
whose fallback silently passes three quarters of a surface and loudly fails the
quarter it read most carefully is worse than one that failed loudly four times.**

The alternative spelling reaches exit 0 and is a route `pantry` does not serve:

```
path: /v1/services/{name}   -> FAIL stale          (the truth, for the wrong reason)
path: /v1/services/name     -> OK                  (a route pantry does not serve)
```

**So the declaration ships at exit 1.** That is a deliberate choice with a price
and the price is stated rather than hidden: anyone running core's checker on
`pantry` gets a red, and the red is a defect in core, not in this tree. Spelling
the path so the tool is satisfied would buy exit 0 with a false row, which is the
defect the whole format exists to prevent. `DECISIONS.md` D34 carries the fix and
who owns it.

---

## 4. `declaration-contradicts-code` cannot see a Go service at all

This is the second finding and it is independent of the first: **even with the
routes visible, `pantry` would not be contradiction-checked.**

`surface.accountKey` is declared in the schema as "what this service calls the
account", and `account_scope_check.py` threads it into exactly **one** of its two
evidence paths — `key_spellings()` → `carries_key`, for `account-key-lost`.
`RESOLVED` and `NAMED`, the tables `declaration-contradicts-code` decides on, are
hardcoded to `account|tenant` **in lower case**. Asked directly:

```
$ cd cafaye/core && python3 - <<'PY'
import sys; sys.path.insert(0, "harness")
from account_scope_check import RESOLVED, NAMED
for line in ["publisherID := ctx.AccountID",
             "publisherID := principal.AccountID",
             "publisherID := principal.account_id",
             "publisherID := currentPublisherID()",
             "pid := params[\"account_id\"]",
             "pid := body.accountId"]:
    fired = bool([p for p in RESOLVED if p.search(line)] or [p for p in NAMED if p.search(line)])
    print("  %-6s %s" % ("FIRED" if fired else "BLIND", line))
PY
  BLIND  publisherID := ctx.AccountID
  BLIND  publisherID := principal.AccountID
  FIRED  publisherID := principal.account_id
  BLIND  publisherID := currentPublisherID()
  FIRED  pid := params["account_id"]
  BLIND  pid := body.accountId
```

Three of those six are the same class of miss, and two of them are the spellings
this file's **own comments cite as its examples**. `RESOLVED`'s comment says "A
context/claim object naming the account — `ctx.AccountID`, `principal.account`",
and `ctx.AccountID` does not match it. `KEY_SPELLINGS` gained `accountID` with
the comment "**the fourth, `accountID`, is Go's** … a checker that could not see
the one language this fleet's substrate is written in would report a scoped route
as unscoped". So `carries_key` was taught Go and `RESOLVED` was not. The same
case-sensitivity misses `body.accountId`, so a caller-NAMED account id in Go or
TypeScript is invisible as well as a resolved one.

**PLANT B — the falsification, three trees, one word different.** Each tree adds
one route in a visible chi spelling, one handler file with one line of body, and
one declaration row that declares it `accountScoped: false`. `principal` is the
Go spelling; `account` is the spelling the checker knows.

```
$ for V in account go publisher; do … ; python3 harness/account_scope_check.py <plantB-$V>; done

--- plantB-account:  publisherID := principal.account_id ---
FAIL account-scope.declaration-contradicts-code: publisher-me (GET /v1/publishers/me) is declared
  accountScoped: false with reason 'public', and the handler resolves an account at
  internal/pantryscope_liar.go:7: `publisherID := principal.account_id` — resolved account
  evidence at internal/pantryscope_liar.go:7
FAIL …/plantB-account: … — 1 failure(s), 1 warning(s) …
(exit 1)

--- plantB-go:  publisherID := principal.AccountID ---
OK …/plantB-go: 0 account-scoped, 5 not-account-scoped, 0 stale, 0 undeclared — 0 failure(s), 1 warning(s) …
(exit 0)

--- plantB-publisher:  publisherID := currentPublisherID() ---
OK …/plantB-publisher: 0 account-scoped, 5 not-account-scoped, 0 stale, 0 undeclared — 0 failure(s), 1 warning(s) …
(exit 0)
```

Three trees, one file, one line of body, one word different, and the same
declaration. Go's casing of a field the checker names in its own comment passes
clean, and **`pantry`'s actual resolver passes clean.** A service that declares
`surface.accountKey: publisher_id` — as this one now does — is no better
protected than one that declares nothing, which is the opposite of what the field
promises.

---

## 5. Falsifying the new test

`internal/httpapi/scope_declaration_test.go` is six tests, and a test that has
never failed has never been proven to test anything. Each plant below is **one
variable**, applied to a throwaway copy of the tree, and each is followed by a
control run of the unplanted copy.

```
$ go test ./internal/httpapi/ -run 'Declared|AccountKey|RegisteredAt|MountedRoute|DeclarationIs|Publisher'
ok  	github.com/cafaye/pantry/internal/httpapi	0.778s        ← control, no plant
```

| plant | what changed, and only that | result |
|---|---|---|
| `row-not-served` | one row added for `POST /v1/phantom`, a route that does not exist | `TestEveryDeclaredEntryPointIsMounted` **FAIL**: `phantom declares POST /v1/phantom, and the router does not serve it.` |
| `undeclared-route` | one route added to `api.gen.go` in the generator's own spelling, no declaration row | `TestEveryMountedRouteIsDeclared` **FAIL**: `the router serves GET /v1/phantom and account_scope.yml does not declare it.` |
| `registeredAt-off-by-one` | one number: `line: 707` → `line: 708` | `TestEveryRegisteredAtLineStillRegistersThatRoute` **FAIL**: `internal/api/api.gen.go:708 does not register GET /v1/services. The line reads: })` |
| `account-key-rotten` | one word: `publisher_id` → `account_id` | `TestTheDeclaredAccountKeyIsTheMigrationsSpelling` **FAIL**: `…the policies do not use it.` |
| `shape-changed` | `registeredAt` flow-mapped to two lines instead of one | `TestTheDeclarationIsWrittenInTheFormThisFileReads` **FAIL**: `list-services has no registeredAt in the shape this file reads` |
| `publisher-call-in-code` | one constant added to `internal/httpapi/httpapi.go`: `const plantedPublisherResolver = "select pantry.begin_publisher($1)"` | `TestNoProductionGoFileResolvesAPublisher` **FAIL**: `…calls begin_publisher in production code.` |
| `publisher-call-in-comment` | **the same call inside a `/* … */` comment** | **PASS** — the control that matters most |

Verbatim, the two that carry the argument:

```
--- plant: undeclared-route ---
--- FAIL: TestEveryMountedRouteIsDeclared (0.00s)
    scope_declaration_test.go:180: the router serves GET /v1/phantom and account_scope.yml does
    not declare it. This is account-scope.undeclared, asserted here because core's recognisers
    cannot read the generated router — see the header of this file.
(exit 1)

--- plant: publisher-call-in-comment ---
--- PASS: TestNoProductionGoFileResolvesAPublisher (0.00s)
ok  	github.com/cafaye/pantry/internal/httpapi	0.640s
```

The comment plant is the one that decides whether the test is worth having.
Every occurrence of `publisher_id` / `current_publisher_id` in this repository's
production Go is in a comment — **seven lines across three files**:

```
$ for f in $(git ls-files 'internal/*.go' 'cmd/*.go' | grep -v '_test\.go$'); do \
    grep -nE "publisher_id|begin_publisher|current_publisher_id" "$f" | sed "s|^|$f:|"; done
internal/pantrydb/db.go:6:// pantry does not add a `where publisher_id = …` to anything, and the rea
internal/pantrydb/db.go:10:// `pantry.current_publisher_id()`. That is the tenant boundary, it is un
internal/pantrydb/db.go:40:// `current_publisher_id()` is NULL and every publisher predicate compare
internal/pantrydb/gen/catalog.sql.go:194:// them filters on `publisher_id` or on `state`, because RL
internal/pantrydb/gen/catalog.sql.go:197:// NO `where publisher_id = …` AND NO `where state = 'publi
internal/pantrydb/gen/querier.go:37:	// them filters on `publisher_id` or on `state`, because RLS al
internal/pantrydb/gen/querier.go:40:	// NO `where publisher_id = …` AND NO `where state = 'published
```

and `internal/pantrydb/db.go:39-40` says "`pantry.publisher_id` is NULL and every
publisher predicate compares against NULL" — *the explanation of why nothing sets
it*. A grep fires on that explanation and stays silent on the call. This is the
same measurement core's `masked()` exists for (`messages_controller.ex` mentions
`account_id` three times and all three are prose), reached from the other
direction: pantry does not need the checker's masking, it needs its own.

### 5.1 The harness must be copied whole, and here is what a partial copy does

```
$ cp core/harness/account_scope_check.py <scratch>/harness-partial/
$ python3 <scratch>/harness-partial/account_scope_check.py <plantB-account>
account_scope_check: cannot import cafaye_contract from …/harness-partial: No module named
'cafaye_contract'. It travels with the harness; a copy of one without the other would mean a
second YAML dialect in core.
(exit 1)
```

It refuses, by design, so the breaking below is a break in a runnable checker.

### 5.2 Deleting the contradiction check from a copy of the harness

The break is on the **call**, not on the check, so the liar passing means
"nothing was found" and not "something raised":

```
$ grep -n "PLANT: the call is deleted" <scratch>/harness-nocontradiction/account_scope_check.py
1427:    contradictions = Contradictions()  # PLANT: the call is deleted

$ python3 <scratch>/harness-whole/account_scope_check.py <plantB-account>          ← unmodified copy
FAIL account-scope.declaration-contradicts-code: publisher-me (GET /v1/publishers/me) is declared
  accountScoped: false with reason 'public', and the handler resolves an account at
  internal/pantryscope_liar.go:7: `publisherID := principal.account_id` …
FAIL …/plantB-account: … — 1 failure(s), 1 warning(s) …
(exit 1)

$ python3 <scratch>/harness-nocontradiction/account_scope_check.py <plantB-account>  ← check deleted
OK …/plantB-account: 0 account-scoped, 5 not-account-scoped, 0 stale, 0 undeclared — 0 failure(s), 1 warning(s)
(exit 0)
```

Same declaration, same repository, same liar. One line of the checker is the
difference between exit 1 and exit 0. That is what makes §4's finding a real
coverage gap rather than a stylistic complaint: on the two spellings the checker
cannot read, the liar is caught **only** by that line, and it is caught by
nothing at all in the copy where the line is gone.

---

## 6. Tallies, kept separate from passes

These are counts of what was written and what was measured. **None of them is a
pass**, and none of them was produced by a test. Each one has the command that
re-derives it.

| tally | value | re-derived by |
|---|---|---|
| entry points declared | 4 | the listing in §3.6, four rows |
| of those `accountScoped: true` | **0** | `account-scope.nothing-scoped: all 4 declared entry point(s) are accountScoped: false` |
| of those `accountScoped: false` | 4 | same line |
| registrations the checker finds in `surface.sources` | **0** | §3.3 |
| registrations the router actually serves | 4 | `TestTheRouterServesTheFourDeclaredOperations` (existing) |
| `surface.minimum` | **0** | `grep -n minimum account_scope.yml` |
| `surface.sources` files named | 1 | the file itself |
| core findings, unplanted tree | 1 failure, 5 warnings | §3.6 |
| core findings, one token deleted | 0 failures, 1 warning | §3.4 |
| `contradiction-unreadable` warnings, unplanted | 4 of 4 rows | §3.6 |
| spellings of an account field the contradiction check reads | 2 of 6 | §4 |
| new Go test functions | 6 | `grep -c '^func Test' internal/httpapi/scope_declaration_test.go` |
| new Go test functions, falsified | 6 of 6 | §5's table, one plant each |
| plants, applied to throwaway copies only | 11 | 1 (`plantA`) + 3 (`plantB-*`) + 7 (`g-*`, one of which is the unplanted control) |

**And the passes, which are a different kind of number:**

```
$ ./bin/prime
checks: 99 run, 99 passed, 0 failed, 0 skipped
skips: 0
rls: 99 of 99 checks passed, 0 tier skipped
…
==> ok
(prime exit 0)

$ grep -cE "^--- PASS" prime.log ; grep -cE "^--- FAIL" prime.log
36
0
```

`tests/rls.sh` is **99 of 99**, unchanged from master, and the Go tier's `36`
`--- PASS` and `0` `--- FAIL` include the six new tests. Nothing in this packet
touched a migration, a query, a handler or the router.

---

## 7. What this repository changed, and what it deliberately did not

**Three files.**

- `account_scope.yml` — four rows, `surface.kind: http`, `minimum: 0`,
  `accountKey: publisher_id`, and a header comment that carries every decision
  above so a reader who opens the file and not this report still gets them.
- `internal/httpapi/scope_declaration_test.go` — six tests. It reads the
  declaration, the generated router and the live router **as text**, with no YAML
  dependency, for the reason `routes_test.go:35-38` gives: "pulling in a YAML
  library to read a document the GENERATOR already parsed would add a dependency
  to the suite for no new assertion". Its `registeredAt` pattern accepts a leading
  `<expression>+`, which is the one thing core's recogniser does not, and that is
  deliberate: a copy of the bug inside the check written to catch it is the
  failure mode the pattern exists to avoid.
- `DECISIONS.md` — `D34`, the open decision, with the option list and the cost of
  not deciding.

**Two things deliberately not done, and why.**

1. **`bin/prime` does not run core's account-scope checker.** It lives in
   `core`, so the tier needs `../core` cloned; `.github/workflows/ci.yml` clones
   `../caf` and not `../core`. Wiring it in means a second clone, a second
   language runtime in the gate, and a green that is a skip on any runner without
   it — the "green over nothing" shape this repository names in its own CI header.
   And on `pantry` the tier would go **red on a correct tree** (§3.6). If a packet
   wires it in, it must wire the clone in the same commit, and the
   `PANTRY_RLS_REQUIRED` / `PANTRY_DB_REQUIRED` pattern says how: prove the thing
   is available before the step that needs it. This is recorded in D34 rather than
   left as an omission.

2. **No row claims a publisher write path**, and none should be added until a flow
   needs one. `migrations/00008` is `with check (false)`; a declaration that
   described a write path the migration removed would be a finding, and
   `declaration-contradicts-code` would eventually be the thing that said so.

**The tree.** One tracked file appended to, two new files, and this report. No
tracked file is modified other than by an append:

```
$ git status --short
 M DECISIONS.md
?? REPORT-pantry-account-scope-01.md
?? account_scope.yml
?? internal/httpapi/scope_declaration_test.go

$ git diff --numstat HEAD -- DECISIONS.md
123	0	DECISIONS.md          ← 123 insertions, 0 deletions: a pure append

$ git diff HEAD -- DECISIONS.md | grep -c '^-[^-]'
0                        ← nothing removed from any tracked file

$ grep -rn "PLANT\|plantedPublisherResolver\|/v1/phantom" internal/ cmd/ account_scope.yml \
    | grep -v 'scope_declaration_test.go'
(no output — no plant is in this worktree)
```

---

## 8. What is still unproven, in one place

**The enumeration is now checked; the verdicts are not, and cannot be from here.**
The six new tests decide whether `account_scope.yml` still describes the router.
They cannot decide whether `reason: public` is *true* of `/v1/services` — that is
`declaration-contradicts-code`'s job, and §4 measures that the job is blind to a
Go-cased field and to the word `publisher`. So the specific gap a phase-2 write
path would open is:

> someone adds a publisher-resolving route. The OpenAPI operation appears in the
> document, `go generate` emits it, and `account_scope.yml` still says
> `reason: public` for every row. The new tests go red on the **missing row**.
> Nothing goes red on a row that is present and **wrong**.

That second half is `DECISIONS.md` D34 §3, and it is core's to close. The three
options there are, in order: teach the chi recogniser one leading concatenation
term (two characters of pattern, fixes the visibility and the false `stale`
together — **the recommendation**); add an OpenAPI-document recogniser so
`surface.sources` can name the spec (strictly more machinery for strictly less
coverage here); and thread `surface.accountKey` into `RESOLVED`/`NAMED` rather
than adding nouns to a list the file keeps saying it does not maintain
(independent of the other two, and it is §4).

**And one thing nobody has measured:** how many services in the fleet generate
their routes rather than writing them. This packet proved the failure mode is real
on one of them. `identity`, `guard`, `site`, `parlor` and `courier` all have
hand-written routers, so the answer for this fleet today is one service, and the
`read_from` field on each of `RECOGNISERS` is a repository somebody read. The
first fleet-wide generated service is not `pantry`.