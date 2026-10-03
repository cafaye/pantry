// Package pantrydb is the connection to the registry's database, and the place the
// ROLE it connects as is decided.
//
// # THE ROLE IS THE POINT OF THIS PACKAGE, AND IT IS CHOSEN ONCE
//
// pantry does not add a `where publisher_id = …` to anything, and the reason is
// not discipline — it is that the database has already decided. `00006_rls.sql`
// puts the catalog's visibility in a POLICY whose qualifier calls
// `pantry.service_is_visible(services)`, which in turn reads
// `pantry.current_publisher_id()`. That is the tenant boundary, it is underneath
// every statement this service issues, and `tests/rls.sh` proves it against a real
// PostgreSQL in 77 checks. A predicate written here would be a second
// implementation of a boundary that already exists, in a layer the RLS suite
// cannot reach, and it could be wrong in the one direction that leaks.
//
// So the question this package answers is not "which rows may I see" — the
// database answers that — it is "which ROLE am I when the database asks". The
// answer is `pantry_public`, for three reasons that are each independently
// sufficient:
//
//	pantry_public is the role with SELECT and nothing else. Not "a policy that
//	happens to deny writes" — there is no write grant to violate. A bug in a
//	policy, a `security definer` function added later, or a membership somebody
//	grants all fail closed here rather than open.
//
//	pantry_admin would see every draft, every unlisted row and every publisher's
//	private work. It exists to be the decision-maker, deliberately not the deploy
//	path (DECISIONS.md D31/D32).
//
//	pantry_publisher is scoped to ONE publisher, chosen by a transaction-local
//	GUC. `/v1/services` is anonymous traffic; there is no identity to set.
//
// # WHY `set role` AND NOT A LOGIN ROLE NAMED pantry_public
//
// `00001_roles.sql` creates `pantry_public` `nologin noinherit`, deliberately, and
// a reader who wants to know "what can log in as pantry_public" should find no
// answer. So the service logs in as `pantry` — which IS a login role, and which
// is `noinherit` too — and takes the group role on every connection. The login
// role on its own can see ZERO catalog rows: with no identity set,
// `current_publisher_id()` is NULL and every publisher predicate compares against
// NULL. That is `FORCE ROW LEVEL SECURITY` working, and it means a connection
// whose `set role` silently failed reads nothing rather than reading everything.
//
// # WHY `AfterConnect` AND NOT "ON CHECKOUT"
//
// Because the role is a property of the CONNECTION, and `AfterConnect` runs once
// per physical connection at the moment it is created. The alternative — issuing
// `set role` per checkout — would leave a window in which a connection is handed
// out with whatever role it last held, and a pool that hands out connections at
// different roles depending on checkout order is a bug that only appears under
// load. Here every connection is the same role from the moment it exists, and the
// only way to change it would be for this process to issue SQL that does, which
// nothing does.
//
// It is also VERIFIED per connection rather than assumed: `AfterConnect` reads
// `current_user` back and refuses the connection if it is not the role that was
// asked for. A role that could not be taken is a connection that never enters
// the pool, so the failure is a pool that cannot be filled — visible at boot and
// on the first request — rather than a query that returns the wrong rows.
package pantrydb

import (
	"context"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/cafaye/pantry/internal/pantrydb/gen"
)

// PublicRole is the role the HTTP service connects as, named once.
//
// It is a constant rather than a variable because the whole package exists to
// make one decision and a variable would make it a deployment's decision, and a
// deployment manifest is not where the tenant boundary is documented. The ingest
// path is a different role — `pantry_admin`, per D32 — and it does not come
// through here.
const PublicRole = "pantry_public"

// connectTimeout bounds establishing the pool. It is a boot-time budget, not a
// retry policy: the packet's rule is that a retry against a database that is
// already failing multiplies the load, and a bounded connect is the honest
// version of that. pgx's own retries stay at their default because they are for
// a connection that was never established.
const connectTimeout = 10 * time.Second

// Store is pantry's handle on the database.
//
// It is a thin wrapper rather than a type with behaviour, and the thinness is the
// claim: the queries are in `queries/catalog.sql`, the generated Go is in `gen/`,
// and this file owns the connection and the role. Nothing here builds SQL.
type Store struct {
	pool *pgxpool.Pool
}

// Open connects and returns the store.
//
// It does NOT retry. A registry whose database is down should say so on its first
// request and be restarted by the orchestrator if that is the deployment's
// choice; a retry loop inside the process converts one outage into continuous
// load against a database that is already failing.
func Open(ctx context.Context, databaseURL string) (*Store, error) {
	if databaseURL == "" {
		return nil, fmt.Errorf("pantrydb: no database URL. PANTRY_DATABASE_URL is this service's whole connection surface")
	}

	cfg, err := pgxpool.ParseConfig(databaseURL)
	if err != nil {
		// The URL is a deployment's own secret-shaped value, so the message names
		// the VARIABLE and not the URL: a password in an error body is a password
		// in somebody's logs.
		return nil, fmt.Errorf("pantrydb: PANTRY_DATABASE_URL is not a URL this driver accepts: %w", err)
	}
	cfg.ConnConfig.RuntimeParams["application_name"] = "pantry"

	// Two steps, and the second is the one that matters. `set role` is issued on
	// the new connection, and then `current_user` is read BACK and compared. A
	// `set role` that silently did nothing — a typo'd role name, a login that is
	// not a member of the group — would otherwise leave a connection in the pool
	// whose privileges are nobody's, and the symptom of that is a catalog that
	// answers 200 with no rows.
	cfg.AfterConnect = func(ctx context.Context, conn *pgx.Conn) error {
		if _, err := conn.Exec(ctx, "set role "+PublicRole); err != nil {
			return fmt.Errorf("pantrydb: could not take the %s role on a new connection: %w", PublicRole, err)
		}
		var current string
		if err := conn.QueryRow(ctx, "select current_user").Scan(&current); err != nil {
			return fmt.Errorf("pantrydb: could not read back the role of a new connection: %w", err)
		}
		if current != PublicRole {
			return fmt.Errorf("pantrydb: a new connection reports current_user %q, not %q: "+
				"the login role is not a member of the group role, so this pool would read nothing",
				current, PublicRole)
		}
		return nil
	}

	ctx, cancel := context.WithTimeout(ctx, connectTimeout)
	defer cancel()

	pool, err := pgxpool.NewWithConfig(ctx, cfg)
	if err != nil {
		return nil, fmt.Errorf("pantrydb: could not open the pool: %w", err)
	}
	// One round trip at boot, so a wrong URL or an absent database is a failed
	// START rather than a service that boots and then answers 503 to everybody.
	// This is the check `/readyz` cannot do for itself, and it is why a wrong
	// `PANTRY_DATABASE_URL` in a deployment manifest is caught by the deploy
	// instead of by the first user.
	if err := pool.Ping(ctx); err != nil {
		pool.Close()
		return nil, fmt.Errorf("pantrydb: could not reach the database: %w", err)
	}

	return &Store{pool: pool}, nil
}

// VerifyRole reads back the role a pooled connection is using and refuses to
// report ready if it is not the one this package set.
//
// It is called once at startup, immediately after Open. `AfterConnect` already
// refuses a connection whose role did not take, so this is belt to that braces —
// and the braces exist because the alternative failure is silent: a pool built
// before a role was revoked, or a `pgxpool.Config` someone edited, and the
// symptom would be a catalog that answers 200 with no rows and a `/readyz` that
// says zero.
func (s *Store) VerifyRole(ctx context.Context) (string, error) {
	var current string
	if err := s.pool.QueryRow(ctx, "select current_user").Scan(&current); err != nil {
		return "", fmt.Errorf("pantrydb: could not read the role of a pooled connection: %w", err)
	}
	if current != PublicRole {
		return current, fmt.Errorf("pantrydb: a pooled connection is using role %q, not %q", current, PublicRole)
	}
	return current, nil
}

// Queries returns the generated read path.
//
// Every method on it is a public catalog read. Nothing in this package writes,
// and nothing in it inserts, updates or deletes: the write path is `pantry_admin`
// and it belongs to the ingest, which is its own packet.
func (s *Store) Queries() *pantrydbgen.Queries { return pantrydbgen.New(s.pool) }

// Close releases the pool. It is safe to call on a nil Store so that a deferred
// Close in main does not need a nil check.
func (s *Store) Close() {
	if s != nil {
		s.pool.Close()
	}
}

// Pool hands out the underlying pool.
//
// It exists for the two things that are not a generated query: the tests, which
// assert what this ROLE can and cannot do (`TestPantryPublicCannotWrite` needs a
// raw statement precisely because it is not one of the catalog's reads), and the
// writer path, which is `pantry_admin` and belongs to the ingest.
//
// The read path goes through Queries rather than through here, so there is one
// place the SQL lives and no way for a caller to reach for a pool and start
// writing catalog SQL by hand.
func (s *Store) Pool() *pgxpool.Pool { return s.pool }

// Ping reports whether the database is reachable right now. `/readyz` uses it so
// that a process which is alive but useless says 503 — see `internal/httpapi`.
func (s *Store) Ping(ctx context.Context) error { return s.pool.Ping(ctx) }
