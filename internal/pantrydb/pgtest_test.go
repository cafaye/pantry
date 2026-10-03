// Package pantrydb_test drives the read path against a real PostgreSQL.
//
// # WHY THERE IS A HARNESS AND WHAT IT REUSES
//
// The claims this package makes — that `ListServices` returns the rows RLS says
// are visible, that `GetService` finds nothing for a name that is not there, and
// that the connection is using `pantry_public` — are not checkable against a
// mock. A mocked driver proves that the mock agrees with the code. Every test
// here therefore runs against a real server with `migrations/` applied and
// `tests/seed.sql` loaded.
//
// The server is NOT started by Go. `tests/rls.sh` already knows how to find a
// PostgreSQL on this machine, already starts a scratch cluster, and already knows
// how to turn the goose directory into an Up-sections script — and a Go copy of
// that knowledge would be a second answer to a question whose whole point is
// that there is one. So `serve()` runs `./tests/rls.sh --serve` and reads the
// connection URL it prints. One harness, two subjects.
//
// # A MISSING POSTGRES IS A COUNTED SKIP WITH A NAMED REASON
//
// `go test` hides `t.Skip` unless it is run with `-v`, and a skipped suite that
// reports `ok` is a suite nobody can tell apart from one that checked nothing.
// So this package prints its own `skip:` rows and a `checks:`/`skips:` summary
// from `TestMain`, on stdout, every run — the same contract `tests/rls.sh`
// already keeps.
package pantrydb_test

import (
	"bufio"
	"context"
	"fmt"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"

	"github.com/cafaye/pantry/internal/pantrydb"
)

// serveTimeout bounds starting a cluster. It is a startup budget and not a retry:
// `initdb` plus six migrations on a laptop is a few seconds, and a suite that
// waits a minute for a database it is never going to get is a suite that hangs.
const serveTimeout = 90 * time.Second

// dbURL is the migrated, seeded database. Empty when there is no PostgreSQL here,
// in which case dbSkip holds the named reason.
var (
	dbURL  string
	dbSkip string
	// ran and skipped count TESTS, not databases, so the summary is comparable
	// with what `go test` itself reports.
	ran, skipped int
)

func TestMain(m *testing.M) {
	var stop func()
	url, skip, stopCluster := serve(repoRoot(), false)
	stop = stopCluster
	dbURL, dbSkip = url, skip
	code := m.Run()
	// AFTER the tests, and not before: the shared cluster is what every test in
	// this package reads, so interrupting it as soon as it printed its URL would
	// kill the database the first test has not finished querying yet. That is not
	// a hypothetical — it is what the first version of this did, and four tests
	// failed with `connection refused` against a cluster that had answered.
	stop()

	// The summary a reader needs in order to trust the run: how many gates were
	// entered, how many of them did not run, and what the gates ran against.
	fmt.Printf("pantrydb: gates: %d entered, %d did not run, exit %d (against %s)\n",
		ran, skipped, code, databaseDescription())
	if dbSkip != "" {
		fmt.Printf("skip: %s\n", dbSkip)
	} else if skipped > 0 {
		fmt.Println("pantrydb: the rows above named every gate that did not run")
	} else {
		fmt.Println("pantrydb: every gate in this package ran against a real PostgreSQL")
	}
	os.Exit(code)
}

func databaseDescription() string {
	if dbSkip != "" {
		return "no database"
	}
	return dbURL
}

// dbGate is the one place a test declares that it needs a database, and the one
// place the counters move.
//
// `t.Cleanup` rather than an increment at the top, because a test that SKIPS never
// returns from the skip and an increment before it would count a gate nobody
// entered. A cleanup DOES run on a skipped test, and `t.Skipped()` is true there,
// so one cleanup counts one gate and counts it as skipped only when it was.
//
// This exists because of a measurement, not a plan. The counters were first
// incremented inline at each call site and read 6 for a run that entered 12 gates,
// and the reason was duller than the bug: `go test` DISCARDS a passing package's
// output entirely, so the `skip:` row this package prints was invisible on every
// green run. `bin/prime-go` now runs the data path with `-v` and echoes these rows;
// see the comment there.
func dbGate(t *testing.T) {
	t.Helper()
	t.Cleanup(func() {
		ran++
		if t.Skipped() {
			skipped++
		}
	})
	if dbSkip != "" {
		t.Skip(dbSkip)
	}
}

// requireStore opens the shared pool, or skips the test with the named reason.
func requireStore(t *testing.T) *pantrydb.Store {
	t.Helper()
	dbGate(t)
	ctx, cancel := context.WithTimeout(context.Background(), serveTimeout)
	defer cancel()
	store, err := pantrydb.Open(ctx, dbURL)
	if err != nil {
		// Not a skip. The harness stood a database up and we could not open it,
		// which is a failure of this package, and reporting it as "no postgres
		// here" would hide it behind the one reason that is allowed to skip.
		t.Fatalf("pantrydb.Open(%s): %v", redact(dbURL), err)
	}
	t.Cleanup(store.Close)
	return store
}

// startEmpty brings up a SECOND, unseeded database and opens it.
//
// It is a second cluster rather than a second database in the first one because
// "--empty" is what `tests/rls.sh` already knows how to do, and because a Go test
// should not be reaching into a cluster to delete the rows it wants absent: the
// empty case is only meaningful if the emptiness is real rather than arranged.
func startEmpty(t *testing.T) *pantrydb.Store {
	t.Helper()
	dbGate(t)
	url, skip, stop := serve(repoRoot(), true)
	if skip != "" {
		t.Skip(skip)
	}
	t.Cleanup(stop)
	ctx, cancel := context.WithTimeout(context.Background(), serveTimeout)
	defer cancel()
	store, err := pantrydb.Open(ctx, url)
	if err != nil {
		t.Fatalf("pantrydb.Open(empty): %v", err)
	}
	t.Cleanup(store.Close)
	return store
}

// serve runs `tests/rls.sh --serve [--empty]` and waits for the URL it prints.
//
// It returns `("", reason)` rather than exiting when there is no PostgreSQL, so
// the caller decides whether that is a skip or a failure — and only this file's
// reason for it is ever printed.
func serve(root string, empty bool) (string, string, func()) {
	script := filepath.Join(root, "tests", "rls.sh")
	args := []string{"--serve"}
	if empty {
		args = append(args, "--empty")
	}

	cmd := exec.Command("bash", append([]string{script}, args...)...)
	cmd.Dir = root
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return "", fmt.Sprintf("tests/rls.sh could not be piped (%v)", err), func() {}
	}
	// stderr into the buffer, NOT into the test log: the harness narrates its
	// progress there and it would drown the failures.
	var errBuf strings.Builder
	cmd.Stderr = &errBuf

	if err := cmd.Start(); err != nil {
		return "", fmt.Sprintf("tests/rls.sh could not be started (%v)", err), func() {}
	}

	urls := make(chan string, 1)
	exited := make(chan error, 1)
	go func() {
		sc := bufio.NewScanner(stdout)
		for sc.Scan() {
			if line := strings.TrimSpace(sc.Text()); strings.HasPrefix(line, "postgres://") {
				select {
				case urls <- line:
				default:
				}
			}
		}
		// Drain the rest so the harness never blocks writing its own narration.
		for sc.Scan() {
		}
		exited <- cmd.Wait()
	}()

	select {
	case url := <-urls:
		// The harness is up and waiting. It is left alone until the caller says
		// otherwise, and the goroutine above owns the reap.
		return url, "", stopWhenAsked(cmd, exited)
	case err := <-exited:
		// The harness finished without printing a URL. On a machine with no
		// PostgreSQL that is its counted skip and exit 0; anything else is its
		// own failure text, which is more useful than anything invented here.
		return "", fmt.Sprintf("tests/rls.sh --serve exited (%v) before standing a database up: %s",
			err, firstLine(errBuf.String())), func() {}
	case <-time.After(serveTimeout):
		_ = cmd.Process.Kill()
		return "", fmt.Sprintf("tests/rls.sh --serve printed no database URL within %s: %s",
			serveTimeout, firstLine(errBuf.String())), func() {}
	}
}

// stopWhenAsked returns the function that shuts the harness down when the caller
// is done with it — and ONLY then.
//
// It sends SIGINT so the harness leaves through its own EXIT trap: server stopped,
// data directory removed. A `Kill` would leave a postmaster behind on a machine
// other sessions are working on, which is the one thing this suite must never do,
// so a SIGKILL is only the backstop for a harness that ignored the polite one.
func stopWhenAsked(cmd *exec.Cmd, exited <-chan error) func() {
	var once sync.Once
	return func() {
		once.Do(func() {
			_ = cmd.Process.Signal(os.Interrupt)
			select {
			case <-exited:
			case <-time.After(20 * time.Second):
				_ = cmd.Process.Kill()
			}
		})
	}
}

func firstLine(s string) string {
	for _, line := range strings.Split(s, "\n") {
		if trimmed := strings.TrimSpace(line); trimmed != "" {
			return trimmed
		}
	}
	return "the harness printed nothing on stderr"
}

// unreachableURL is a URL to a port nothing is listening on.
//
// It is how the "database is down" case is tested WITHOUT taking the real one
// away, which matters twice over: a test that proved 503 by stopping the shared
// database would also make every other test in this package unable to run, and a
// 503 observed in a package where nothing else worked would not be evidence that
// the 503 is specific to an unreachable database. This is a second, deliberately
// dead connection — the same failure a real outage produces — beside a healthy
// one that keeps answering.
func unreachableURL(t *testing.T) string {
	t.Helper()
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("unreachableURL: no free port to abandon: %v", err)
	}
	// Bound and then CLOSE it: the port is now unbound, which is the point. A
	// refused connection is the fastest failure a TCP endpoint has, and it is what
	// an orchestrator sees when a database is gone.
	addr := l.Addr().String()
	if err := l.Close(); err != nil {
		t.Fatalf("unreachableURL: closing the probe listener: %v", err)
	}
	return "postgres://postgres@" + addr + "/pantry?sslmode=disable&connect_timeout=1"
}

// repoRoot is the directory holding go.mod. Walking up from the test's own
// working directory is what makes the harness independent of where `go test` was
// invoked from — the difference between a suite a developer can run with `-C` and
// one they cannot.
// direct runs one query on a connection of its own, with the catalog role NOT
// taken, and returns its row.
//
// It exists because of one specific and important asymmetry: every connection the
// service opens has `pantry_public` on it, so a test cannot use the service's own
// pool to look at the rows the service cannot see. That asymmetry IS the finding —
// the same database, the same credentials, answers "four rows" on a connection
// that took no role and "zero rows" on one that took the read role — and measuring
// both halves needs a second connection.
//
// It is deliberately NOT `pool.Reset()` after a `set role` reset. That would leave
// a connection in the pool holding whatever role it was left with, which is the
// exact bug `AfterConnect` exists to make impossible.
func direct(ctx context.Context, t *testing.T, sql string) pgx.Row {
	t.Helper()
	conn, err := pgx.Connect(ctx, dbURL)
	if err != nil {
		t.Fatalf("connecting directly to %s: %v", redact(dbURL), err)
	}
	t.Cleanup(func() { _ = conn.Close(context.Background()) })
	return conn.QueryRow(ctx, sql)
}

// asRole runs one statement as a named role on a connection of its own, for the
// checks that need a role other than the catalog's.
func asRole(ctx context.Context, t *testing.T, role, sql string) pgx.Row {
	t.Helper()
	conn, err := pgx.Connect(ctx, dbURL)
	if err != nil {
		t.Fatalf("connecting directly to %s: %v", redact(dbURL), err)
	}
	t.Cleanup(func() { _ = conn.Close(context.Background()) })
	if _, err := conn.Exec(ctx, "set role "+role); err != nil {
		t.Fatalf("taking the %s role: %v", role, err)
	}
	return conn.QueryRow(ctx, sql)
}

func repoRoot() string {
	dir, err := os.Getwd()
	if err != nil {
		return "."
	}
	for {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "."
		}
		dir = parent
	}
}

// redact removes anything password-shaped from a URL before it goes in a failure
// message. None of the suite's own URLs have one; this is for the day somebody
// points a test at a deployment's.
func redact(url string) string {
	at := strings.LastIndex(url, "@")
	scheme := strings.Index(url, "://")
	if at < 0 || scheme < 0 || at < scheme {
		return url
	}
	return url[:scheme+3] + "…" + url[at:]
}
