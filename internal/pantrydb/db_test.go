package pantrydb_test

import (
	"context"
	"strings"
	"testing"
	"time"

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

// TestTheCompatibilityGraphIsUnreadableByTheCatalogRole is the most important
// measurement in this packet, and it asserts a DEFECT on purpose.
//
// Measured on PostgreSQL 18.4 against the merged migrations, with the seed
// applied: `pantry.service_compat` holds four rows, and `select count(*) from
// pantry.service_compat` as `pantry_public` answers ZERO. The cause is not the
// grant — `00006_rls.sql` grants SELECT on that table to `pantry_public` — it is
// that no SELECT policy on `service_compat` names `pantry_public`. The four
// policies that exist there are the two publisher-shaped sets (`pantry`,
// `pantry_publisher`) and the four admin sets (`pantry_admin`).
//
// So the compatibility graph is invisible to the only role the catalog reads as.
// The query in `queries/catalog.sql` is correct and the join works; there is
// nothing on the other side of it. Until a policy is added, this registry can
// answer "what is similar" and cannot answer "what composes", which is the one
// claim `00004_service_compat.sql` makes about itself.
//
// THIS TEST IS A TRIPWIRE, and that is why it asserts the defect rather than
// skipping. It reads `pg_policies` for the mechanism, so the day somebody adds
// `service_compat_public_read` this test goes RED and says the graph has become
// readable — which is the moment `RequirementsForService` can be put on the wire,
// and `REPORT-registry-pantry-data-01.md` names as the single next move. A test
// that skipped here would let the gap outlive the packet that found it.
func TestTheCompatibilityGraphIsUnreadableByTheCatalogRole(t *testing.T) {
	store := requireStore(t)

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	// The rows are there. Measured on a connection that did NOT take the role,
	// because the pooled one deliberately has — see `asRole`.
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
	if len(edges) != 0 {
		t.Fatalf("muse's requirements came back as %d edges through pantry_public.\n"+
			"  That means a SELECT policy naming pantry_public now exists on service_compat, and "+
			"this test has done its job: RequirementsForService is reachable and can be put on "+
			"the wire. Move the assertion in this file over to TestTheCompatibilityGraphIsReachable "+
			"and delete this one — do not leave both.", len(edges))
	}

	// THE MECHANISM, not just the outcome, because "zero rows" is also what an
	// empty table looks like and what a wrong join looks like. Asserting the
	// missing policy is what makes this a diagnosis rather than an observation.
	var policies int
	err = store.Pool().QueryRow(ctx,
		`select count(*) from pg_policies
		  where schemaname = 'pantry' and tablename = 'service_compat'
		    and cmd = 'SELECT' and roles @> array['pantry_public']::name[]`).Scan(&policies)
	if err != nil {
		t.Fatalf("reading pg_policies: %v", err)
	}
	if policies != 0 {
		t.Fatalf("a SELECT policy naming pantry_public now exists on service_compat, and yet " +
			"RequirementsForService returned no rows. The missing policy was NOT the cause; read " +
			"the visibility of the edge TARGET next, because 00006 says an edge is visible " +
			"exactly when its target service is.")
	}

	// And the grant IS there, so this is a policy gap and not a permission gap —
	// which is the difference between a one-statement fix and a migration that
	// widens somebody's privileges.
	var granted bool
	if err := store.Pool().QueryRow(ctx,
		"select has_table_privilege('pantry_public', 'pantry.service_compat', 'SELECT')").Scan(&granted); err != nil {
		t.Fatalf("reading has_table_privilege: %v", err)
	}
	if !granted {
		t.Fatal("pantry_public does not hold SELECT on service_compat either, so the fix is " +
			"the grant as well as the policy. 00006_rls.sql grants it, so this would mean the " +
			"migration that does has not been applied.")
	}
	t.Logf("service_compat: %d rows, 0 readable by pantry_public — a SELECT policy naming it "+
		"does not exist, and the grant is present", total)
}
