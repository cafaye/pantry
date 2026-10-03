# REPORT — registry-compat-05

**Repo:** `cafaye/pantry` · **Branch:** `worker/reg-compat-05` · **Base:** `2f3b623`

**The packet in one line:** the compatibility graph — the reason this registry is
not a link directory — goes on the wire, as two operations with two answer
shapes, and the decision that two packets deferred is made in `DECISIONS.md` D35.

`GET /v1/services/{name}/requirements` — *what do I need to run this?*
`GET /v1/services/{name}/required-by` — *who breaks if I change this?*

---

## 1. How the work started: two comments that had stopped being true

The catalog's package comment said the graph's query was "unreachable today
because no SELECT policy on `service_compat` names `pantry_public`", asserted by
a test named `TestTheCompatibilityGraphIsUnreadableByTheCatalogRole`. Neither
statement was true any more: `00007_roles_and_compat_read.sql` wrote the policy,
and `db_test.go` carries `TestTheCompatibilityGraphIsReachableByTheCatalogRole` —
"the test that the unreadability test asked to be written". The reachability
packet moved the mechanism and left the prose behind, so a reader of
`catalog.go` would have concluded the graph was months away when it was one
contract change away.

Fixed here, and the fixed text records what the graph *is* rather than what it
was waiting for. The lesson is the one `registry-norust-03` already paid for in a
different currency: **a comment describing a state is a claim that needs the same
maintenance the code needs**, and prose that names a mechanism outlives the
mechanism it names.

## 2. The contract decision, made and recorded

`REPORT-registry-pantry-data-01.md` ended by asking for a deliberate decision:
field on `Service`, or route? **D35 rules: two routes.** The short form:

1. The two directions are two questions asked by two different people — an
   operator choosing what to run, an operator deciding whether a change is safe.
2. A field on `Service` would load graph data into every list row, making the
   common case pay for a question it is not asking.
3. Two wrappers make "which question did I ask?" a type in every generated
   client. Confusing *what must I run?* with *what must I not run alongside?* is
   how a caller installs a conflict; a type error is cheaper than that outage.

Also ruled, because it was hiding inside the same decision: **`requires` edges
only**, on both routes. `conflicts_with` has no operation in this version; when
it gets one it gets a third method and a third wrapper. The catalog's
`Requirements` comment states the boundary, and nothing in the interface can
reach a conflict edge.

**What it costs:** `cafaye-ts`'s generated client is stale from this merge, and
regenerating it is part of landing rather than a follow-up — that is the next
packet (`registry-ts-compat-06`).

## 3. The bug my own test found, and the layer where it lived

The first version of `Postgres.Requirements` ran the edge query and returned the
rows. The contract tests then failed:

```
--- FAIL: TestTheGraphRoutesAnswer404ForAnUnknownName
    GET /v1/services/nope/requirements is 200, want 404
```

The edge query answers **zero rows identically for a leaf and for a name that
was never registered**. Through the real database, a typo'd name would have been
answered `200 {"data": []}` — "here is a service with no dependencies" — and the
client would go and try to run it. The document says unknown name is `404` on
both graph routes; nothing made that true.

The fix checks the subject through `Get` before the edge query runs, and the
reason it goes through `Get` rather than a second existence query is in the
comment: **one definition of existence**, the same one the service route uses. A
second query answering the same question is a second answer that can disagree
with the first. The cost is one extra round trip per graph request, and the
cheap alternative was a second definition of "exists" in the read path.

The interface contract now states the sentinel for both methods, and BOTH
implementations enforce it: the fake checks its `known` list, the real catalog
checks through `Get`. That is deliberate — the HTTP contract tests run against
fakes, so a behavior that lived only in `Postgres` would be one a fake could
silently not have, and the handler tests would certify a 200 no real database
can produce.

## 4. Four mutations, and what the fourth taught

Every mutation applied, run, reverted; each caught by a named test.

| | mutation | caught by |
|---|---|---|
| M1 | the `required-by` handler calls `Requirements` — one "simplification" and the two questions are one | `TestRequiredByIsTheOtherQuestionAndNotTheSameAnswer` |
| M2 | `Postgres.RequiredBy` stops checking the subject first | `TestTheGraphRoutesRefuseANameTheRegistryDoesNotCarry` |
| M3 | the edge's `dependency` word is flattened to `required` | `TestTheBackwardDirectionOfTheGraphIsTheMirrorOfTheForward` |
| M4 | the wrapper stops echoing the subject | `TestRequirementsIsTheForwardDirectionOfTheGraph` |

**M3 did not go red the first time, and the reason is worth keeping.** The
assertion read the edge through `store.Queries()` — the raw generated queries —
while the mutation sat in `catalog.RequiredBy`'s mapping, the function the wire
actually goes through. The test passed over a flattened `dependency` word, which
is exactly the failure the assertion existed for: "runs degraded without it"
turned into "does not start without it", invisible. The test now reads through
`catalog.New(store.Queries())` for that half, and M3 goes red. **A test that
asserts at the wrong layer is a green light over the bug it names.**

## 5. The two directions are not symmetric, and the asymmetry is asserted

The backward query is the mirror of the forward one, and the one line that
differs in a mirror is the line most likely to be wrong: the visibility
predicate sits on the **requiring** side here and on the **target** side there.
`TestTheBackwardDirectionOfTheGraphIsTheMirrorOfTheForward` pins the exact
backward sets for three seed shapes — a service two others require (`identity`),
one required by a published and a soft third-party edge (`courier`), and a leaf
(`caf`) — plus the soft edge's word, by name, through the catalog.

The document names the one consequence of the both-endpoints rule in this
direction: an edge **from** a draft service is hidden, so a public backward
graph grows as work in progress is published. That is the policy's answer, it is
the conservative one, and the route's description says so rather than leaving a
reader to discover it from a missing row.

## 6. Interleaving, floors, and the other session's packet

This worktree was cut from `2f3b623` — the other session's
`worker/pantry-publisher-rewrite-01` had merged while the previous packet was
closing, and it added ten RLS checks without moving the `rls` floor. The
discipline the gate file itself states is that **a floor moves in the commit
that changes the tests it counts** — a floor left behind is a margin nobody
decided to open. So this packet carries both moves:

- `suite` 30 → **36** (six new tests: two direction tests, one 404, one leaf,
  one real-catalog refusal, one backward mirror) — margin still zero.
- `rls` 89 → **99** (the publisher-identity packet's ten, measured at 99 run /
  99 passed) — margin restored to zero.

One test was **renamed**, not added:
`TestTheRouterServesTheFourDeclaredOperations` →
`TestTheRouterServesEveryDeclaredOperation`. The document declares six
operations now, and a test whose name carries a number goes stale as a lie
rather than as a failure — it keeps passing while asserting a census two behind.
`bin/gate-self-test`'s one-test case anchors on that name, so the anchor moved in
the same commit; the case's own comment says a named test disappearing must be
visible, and this was that event, caught before it could surprise.

## 7. What landed

- `openapi/v1.yaml` — two operations, three schemas (`CompatibilityEdge`,
  `ServiceRequirements`, `ServiceRequiredBy`), each 404/503 documented with the
  same problem envelope as the service routes.
- `internal/pantrydb/queries/catalog.sql` — `RequiredByForService`, mirrored from
  the forward query with the visibility predicate on the requiring side; the
  forward query's "not on the wire yet" note replaced by the decision it was
  waiting for.
- `internal/catalog` — `Requirements`/`RequiredBy` on the `Catalog` interface in
  the document's generated types; the pantry-owned `Requirement` type deleted,
  because a second edge shape beside the document's is a second description that
  can drift.
- `internal/httpapi` — two handlers over one shared body, with the direction a
  label rather than a branch; `ServiceRequirements` and `ServiceRequiredBy` stay
  distinct generated types.
- README — the two operations, their shapes, the both-endpoints rule, the leaf
  and the typo as different answers, and `requires`-only explained.

## 8. Verification

```
./bin/prime         → suite: 36 passed, 0 failed
                      rls: 99 of 99 checks passed, 0 tier skipped
                      data path: every check ran against a real PostgreSQL (0 skipped)
                      ==> ok
./bin/gate-self-test → 37 breakages, 9 warning cases, 0 failure(s), 0 skipped
four mutations      → each caught by the named test, each reverted, tree clean
```

**One transient, recorded rather than buried.** The FIRST self-test run reported
3 failures. Two were the stale `minimum: 30` anchor in the `suite-floor-too-high`
case — my own floor move had gone stale in the case that proves the floor, and
the fix keeps the anchor with a comment: a breakage that edits the declaration
by literal is coupled to the declaration's constants. The third was
`manifest-tier-stood-down` going red with `gate.proof-missing` on `data-path` —
a case this run's siblings passed and an isolated reproduction passed twice. It
did not reproduce on two subsequent full runs, and the suspected cause is a port
overlap between back-to-back prime runs: both the Go tier's cluster and the RLS
tier's bind the same fixed port, sequentially within one run, and the self-test
runs dozens of runs back to back. **A failure that cannot be reproduced is not a
failure explained**, so it is named here with its suspicion attached rather than
made to vanish. The self-test's work dir is now kept when
`PANTRY_GATE_KEEP_WORK=1`, because a failing proving case deletes its own
evidence — the gate.log that explains the red is inside the directory the exit
trap removes — and diagnosing that meant re-running with the trap patched, which
is busywork a diagnostic tool should not create.

## 9. The single next move

**Regenerate `cafaye-ts` from this document** (`registry-ts-compat-06`), before
any other packet touches the contract. The generated client is the thing the
graph decision deliberately deferred to, and a stale client beside a new
operation is exactly the drift the vendored-schema header says this fleet does
not paper over.
