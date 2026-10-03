package pantrydb_test

import (
	"context"
	"errors"
	"sort"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgconn"

	"github.com/cafaye/pantry/internal/catalog"
	"github.com/cafaye/pantry/internal/pantrydb"
)

// TestTheConnectionReadsAsPantryPublic is the assertion the whole packet rests
// on, and it is an assertion about `current_user` rather than about a row count
// because the row count cannot tell you WHY it is what it is.
//
// A pool that silently failed to take the role would still run these queries, and
// `pantry` on its own reads ZERO catalog rows — so a count of zero would look
// like an empty database rather than like a broken pool. Reading the role back is
// the only check that separates the two, and `Open` already refuses a connection
// whose role did not take, so this is the assertion that the refusal is real
// rather than a line of code that looks like one.
func TestTheConnectionReadsAsPantryPublic(t *testing.T) {
	store := requireStore(t)

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	role, err := store.VerifyRole(ctx)
	if err != nil {
		t.Fatalf("VerifyRole: %v", err)
	}
	if role != pantrydb.PublicRole {
		t.Fatalf("a pooled connection is using role %q, want %q.\n"+
			"  Every visibility claim this package makes is a claim about RLS, and RLS is "+
			"evaluated for the CURRENT role. A connection that is not %q is not reading the "+
			"catalog — it is reading whatever its own policies allow, which for `pantry` with "+
			"no identity is nothing at all.",
			role, pantrydb.PublicRole, pantrydb.PublicRole)
	}
}

// TestOneQueryAgainstTheRealServicesTable is the smallest true version of "the
// 503 becomes a 200": a statement over `pantry.services`, run as the service's
// role, against a database that has the migrations applied.
func TestOneQueryAgainstTheRealServicesTable(t *testing.T) {
	store := requireStore(t)

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	count, err := store.Queries().CountServices(ctx)
	if err != nil {
		t.Fatalf("CountServices: %v", err)
	}
	if count == 0 {
		t.Fatal("CountServices returned 0 against a seeded database.\n" +
			"  The query is `select count(*) from pantry.services` with no WHERE clause at all, " +
			"so a zero here means RLS is hiding every row — which is what `pantry` sees when the " +
			"role was not taken, and is exactly why the role assertion above runs first.")
	}
	t.Logf("CountServices: %d visible services", count)
}

// TestPantryPublicCannotWrite is the other half of "the role decides", and it is
// here rather than in `tests/rls.sh` for one reason: the RLS suite proves the
// GRANT exists on a cluster it built with `set role`, and this proves the SERVICE
// cannot reach a write through the connection it actually opens. A pool is a
// connection like any other, and the property that matters is the one a caller
// would observe.
//
// The failure expected is the GRANT boundary (`permission denied for table`), not
// the policy boundary, and asserting which one is the point: `pantry_public`
// holds no INSERT privilege anywhere, so Postgres refuses before a policy is ever
// consulted. A suite that asserted only "it failed" would pass just as well
// against a role whose write policies were all `using (false)` — and that is a
// materially weaker claim.
func TestPantryPublicCannotWrite(t *testing.T) {
	store := requireStore(t)

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	conn, err := store.Pool().Acquire(ctx)
	if err != nil {
		t.Fatalf("acquire: %v", err)
	}
	defer conn.Release()

	_, err = conn.Exec(ctx,
		`insert into pantry.services (name, language, kind, trust, state, core_constraint, manifest, manifest_sha256)
		 values ('injected', 'go', 'api', 'third_party', 'published', '^0.1.0', '{}'::jsonb, repeat('0', 64))`)
	if err == nil {
		t.Fatal("an INSERT through the service's own connection succeeded.\n" +
			"  The service reads the catalog and nothing else; a role that can write it is a " +
			"role that can publish a service, and publishing is a reviewed decision, not a query.")
	}
	for _, want := range []string{"permission denied for table services", "42501"} {
		if !strings.Contains(err.Error(), want) {
			t.Fatalf("the INSERT failed, but not at the barrier this check is about.\n"+
				"  want the error to mention %q (the GRANT boundary), got: %v\n"+
				"  A write refused by a POLICY is a different claim from a write refused by a "+
				"missing grant, and only the second is guaranteed by `00006_rls.sql`'s grant list "+
				"however the policies are later edited.",
				want, err)
		}
	}
}

// TestAnUnreachableDatabaseIsAnErrorAndNotAZero is the middle case of the three
// the 503 exists to keep apart, at the connection layer rather than the HTTP
// layer.
//
// A database that is DOWN must not be reachable-as-zero. If `Open` could return a
// store whose queries answer 0 — a pool that swallows the dial failure, a nil
// connection treated as an empty result — then `GET /v1/services` would answer
// 200 with `services: []` for an outage, which is precisely the confusion the
// 503 was written to prevent. The assertion is that `Open` REFUSES, because a
// store that cannot reach its database is not a store.
func TestAnUnreachableDatabaseIsAnErrorAndNotAZero(t *testing.T) {
	url := unreachableURL(t)

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	store, err := pantrydb.Open(ctx, url)
	if err == nil {
		store.Close()
		t.Fatalf("Open(%s) succeeded against a port nothing is listening on.\n"+
			"  This is the failure the 503 exists for: a store that opens against a dead database "+
			"turns an outage into an empty catalog, and an empty catalog is a 200 a client caches.",
			redact(url))
	}
	if !strings.Contains(err.Error(), "could not reach the database") {
		t.Fatalf("Open failed, but not for the reason this test is about: %v", err)
	}
	t.Logf("Open on an unreachable database: %v", err)
}

// TestTheCompatibilityGraphIsReachableByTheCatalogRole is the test that
// TestTheCompatibilityGraphIsUnreadableByTheCatalogRole asked to be written
// instead of itself, on the day it stopped being true.
//
// That test asserted a defect on purpose, as a tripwire: it read `pg_policies`
// for the mechanism so that adding `service_compat_public_read` would turn it
// RED and name the moment the graph became readable. It did, and it said to
// move this assertion here and delete it. This is that, and it is the only
// reason the defect had a shelf life rather than becoming folklore.
//
// WHAT CHANGED. `migrations/00007_roles_and_compat_read.sql` writes
// `00006_rls.sql`'s own argument as a statement — "an edge is visible exactly
// when its `target_id` service is visible" — which `00006` argued for at length
// and declined to write. Before it: `pantry.service_compat` held four rows,
// `pantry_public` held the SELECT grant, and no SELECT policy named it, so the
// compatibility graph was invisible to the only role the catalog reads as. The
// query generated by sqlc was correct the whole time. There was nothing on the
// other side of the join.
//
// WHAT IS ASSERTED, and why each part. The count, because "readable" and
// "happens to be empty" are the same answer and only one of them is a pass. The
// exact edge set, because a policy that let everything through would also make
// this count non-zero, and a graph that exposes an edge into a draft is the
// failure `00006` spends a paragraph preventing. And the policy's existence,
// read from `pg_policies`, because the outcome alone cannot tell a correct
// policy from a correct answer reached for the wrong reason.
func TestTheCompatibilityGraphIsReachableByTheCatalogRole(t *testing.T) {
	store := requireStore(t)

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	// The rows are there, measured WITHOUT the catalog role — on a connection
	// that did not `set role`, because the pooled one deliberately has. This is
	// what makes the counts below a comparison rather than two restatements.
	var total int
	if err := direct(ctx, t, "select count(*) from pantry.service_compat").Scan(&total); err != nil {
		t.Fatalf("counting service_compat without the catalog role: %v", err)
	}
	if total == 0 {
		t.Fatal("service_compat is empty, so this test would pass for the wrong reason: an " +
			"empty graph and a hidden graph look identical from the catalog. The seed inserts four.")
	}

	edges, err := store.Queries().RequirementsForService(ctx, "muse")
	if err != nil {
		t.Fatalf("RequirementsForService(muse): %v", err)
	}
	if len(edges) == 0 {
		t.Fatalf("muse has %d edges in the database and RequirementsForService returned none "+
			"through pantry_public.\n  00007 grants the policy, so this means it has not been "+
			"applied, or that every one of muse's edges has an invisible endpoint.", total)
	}

	// THE EXACT SET. `muse` requires `identity` and `courier`, and both are
	// published first-party rows, so both edges are public. An assertion that
	// only counted them would pass just as well for a policy that leaked every
	// edge in the table — which is the leak `00006` argues against and the one
	// this number exists to make visible.
	var names []string
	for _, e := range edges {
		names = append(names, e.TargetName)
	}
	sort.Strings(names)
	if got, want := "["+strings.Join(names, " ")+"]", "[courier identity]"; got != want {
		t.Errorf("muse's requirements are %s through pantry_public, want %s.\n"+
			"  An edge is public exactly when BOTH its endpoints are visible. An extra name here "+
			"is an edge pointing at a service the public catalog does not list — the partial map "+
			"00006_rls.sql refuses to publish.", got, want)
	}

	// THE MECHANISM, because a correct answer can arrive by accident. If this
	// count is zero the rows are visible through something other than the policy
	// this migration added — a superuser connection, or a table owner — and the
	// boundary is not where the schema says it is.
	var policies int
	if err := store.Pool().QueryRow(ctx,
		`select count(*) from pg_policies
		  where schemaname = 'pantry' and tablename = 'service_compat'
		    and cmd = 'SELECT' and roles @> array['pantry_public']::name[]`).Scan(&policies); err != nil {
		t.Fatalf("reading pg_policies: %v", err)
	}
	if policies != 1 {
		t.Errorf("there are %d SELECT policies naming pantry_public on service_compat, want "+
			"exactly 1. One per command, always — two would be two independent statements "+
			"claiming to decide the same question.", policies)
	}
	t.Logf("service_compat: %d rows in the database, %d readable by pantry_public (%v)",
		total, len(edges), names)
}

// TestTheCatalogRoleCanStillNotWrite is here because the policy added in 00007
// is a SELECT, and the cheapest way for a migration to turn a read path into a
// write path is to be careless about which commands it names. The graph is
// publisher claims: an edge is a publisher's statement about its own service,
// and `pantry_public` is nobody's publisher.
func TestTheCatalogRoleCanStillNotWrite(t *testing.T) {
	store := requireStore(t)

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	// At the GRANT boundary, not a policy. 00007 adds a SELECT policy and no
	// grant of anything else, so there is no privilege for a policy to refuse —
	// which is a stronger statement than "a policy denied it", and one worth
	// keeping true.
	//
	// The column list is `kind, version_range`, not the `constraint` the comment
	// above first reached for: `constraint` is a reserved word in Postgres and
	// the statement dies of 42601 before privilege is ever consulted, which is a
	// refusal that looks like a refusal and proves nothing.
	_, err := store.Pool().Exec(ctx,
		"insert into pantry.service_compat (service_id, target_id, kind, version_range, dependency) "+
			"select id, id, 'requires', '^0.1.0', 'required' from pantry.services limit 1")
	if err == nil {
		t.Fatal("pantry_public inserted a compatibility edge.\n" +
			"  00007 adds SELECT and nothing else. If this fails at a POLICY rather than at a " +
			"grant, a policy was widened that should not have been.")
	}
	var pgerr *pgconn.PgError
	if !errors.As(err, &pgerr) || pgerr.Code != "42501" {
		t.Fatalf("the refusal was %v, want SQLSTATE 42501 (insufficient_privilege) — the grant "+
			"boundary, not a policy. A policy refusal is 42501 too but names a policy, and the "+
			"two are different guarantees.", err)
	}
	t.Logf("write refused at the grant: %v", pgerr.Message)
}

// TestTheBackwardDirectionOfTheGraphIsTheMirrorOfTheForward exists because
// `RequiredByForService` was written by mirroring `RequirementsForService`, and
// the one line that differs in a mirror is the line most likely to be wrong:
// the visibility predicate has to sit on the REQUIRING side here, or an edge
// from a draft service into a published one would be public in this direction
// while its mirror image is hidden in the other. The graph would then depend on
// which way you read it, which is not a property a map can have.
//
// The seed gives this test its fixtures and every one of them is load-bearing:
// courier→identity and muse→identity are the backward edges of `identity`;
// muse→courier and parcel→courier (a SOFT edge, and a third-party publisher's,
// which exercises a different trust row) are the backward edges of `courier`;
// and `caf` is the leaf nothing requires.
func TestTheBackwardDirectionOfTheGraphIsTheMirrorOfTheForward(t *testing.T) {
	store := requireStore(t)

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	for _, tc := range []struct {
		subject string
		want    string
	}{
		{"identity", "[courier muse]"},
		{"courier", "[muse parcel]"},
		// The leaf, and it must be measured rather than assumed: an empty
		// answer is also what a broken query returns, so this row only has
		// value because the two cases above prove the query is not broken.
		{"caf", "[]"},
	} {
		rows, err := store.Queries().RequiredByForService(ctx, tc.subject)
		if err != nil {
			t.Fatalf("RequiredByForService(%s): %v", tc.subject, err)
		}
		var names []string
		for _, r := range rows {
			names = append(names, r.RequirerName)
		}
		sort.Strings(names)
		if got := "[" + strings.Join(names, " ") + "]"; got != tc.want {
			t.Errorf("required-by(%s) is %s, want %s.\n  An extra name is an edge this direction "+
				"leaks that the forward direction hides; a missing one is an edge the policy hides "+
				"that the seed publishes.", tc.subject, got, tc.want)
		}
	}

	// THE SOFT EDGE, BY NAME, and through the CATALOG rather than the store,
	// which is the difference the first draft of this test got wrong: the store's
	// rows carry whatever the database said, so a mapping bug in
	// `catalog.RequiredBy` — the function the wire actually goes through — would
	// have been invisible here. `dependency` is a third field on the edge and the
	// forward test never sees a non-`required` value; parcel→courier is the
	// seed's one soft edge, and if it arrives as `required`, the graph has turned
	// "runs degraded without it" into "does not start without it", which is the
	// difference between a warning and an outage.
	courierEdges, err := catalog.New(store.Queries()).RequiredBy(ctx, "courier")
	if err != nil {
		t.Fatalf("catalog.RequiredBy(courier): %v", err)
	}
	seen := false
	for _, e := range courierEdges {
		if e.Name == "parcel" {
			seen = true
			if e.Dependency != "soft" {
				t.Errorf("parcel's edge to courier reads dependency %q through the catalog, "+
					"want \"soft\" — the mapping is flattening the one word that separates "+
					"a degraded start from no start", e.Dependency)
			}
		}
	}
	if !seen {
		t.Error("parcel is not in required-by(courier) through the catalog, so the soft edge " +
			"was not checked at all — the first half of this test must have failed already")
	}
}

// TestTheGraphRoutesRefuseANameTheRegistryDoesNotCarry is the half the fake in
// `internal/httpapi` cannot certify. The handler's 404 comes from
// `catalog.ErrNoSuchService`, and `Postgres` produces that sentinel by checking
// the subject through `Get` BEFORE the edge query runs — because the edge query
// answers zero rows identically for a leaf and for a name that was never
// registered, and a 200 with `data: []` on a typo is a client going off to run
// a service that does not exist. The fake implements the same contract by its
// `known` list, so the handler tests above pass over both; this test is the
// one that runs the REAL catalog and would catch the day the Get-first check
// is "simplified" out of it.
func TestTheGraphRoutesRefuseANameTheRegistryDoesNotCarry(t *testing.T) {
	store := requireStore(t)
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	// Through the CATALOG, not the store: the sentinel is a catalog-layer
	// contract and the edge queries on the store return row slices, so calling
	// them here would test the wrong layer's idea of the answer.
	cat := catalog.New(store.Queries())
	if _, err := cat.Requirements(ctx, "nope"); !errors.Is(err, catalog.ErrNoSuchService) {
		t.Errorf("Requirements(nope) is %v, want catalog.ErrNoSuchService — an empty graph "+
			"is the answer for a LEAF, and a typo is not a leaf", err)
	}
	if _, err := cat.RequiredBy(ctx, "nope"); !errors.Is(err, catalog.ErrNoSuchService) {
		t.Errorf("RequiredBy(nope) is %v, want catalog.ErrNoSuchService", err)
	}
}
