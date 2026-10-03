// Command pantry serves the cafaye service registry.
//
// This is the Go rewrite, standing beside the Rust implementation in `src/`.
//
// # CONFIGURATION, and the three variables that are the service's whole surface
//
//	PANTRY_BIND           listen address, default 0.0.0.0:8080
//	PANTRY_LOG_LEVEL      slog level, default info
//	PANTRY_DATABASE_URL   the registry's PostgreSQL, and the only variable that
//	                      decides whether this process can serve anything
//
// There is no registry directory and no flag set, and the absence is the point: a
// variable this build ignores is a lie in a deployment manifest.
//
// # A MISSING OR UNREACHABLE DATABASE DOES NOT STOP THE PROCESS, AND THAT IS A
// DECISION RATHER THAN A DEFAULT
//
// Both are logged at error level, both leave the service running with no catalog
// mounted, and both answer 503 on the data routes with a `detail` naming which of
// the two it was — while `/healthz` keeps answering 200.
//
// The document states the rule for `/healthz` in its own words ("200 whenever the
// process is running — including when the registry did not load"), and the reason
// it states it is the one that applies here: an orchestrator that restarts on a
// data problem turns a database outage into a crash loop, which is load on a
// database that is already failing. So the process starts, says what it could not
// do, and lets `/readyz` be the probe that reports it — which is exactly what two
// probes are for.
//
// The cost of that choice, stated rather than discovered later: a deployment that
// has lost its database stays in the load balancer until something restarts it,
// and reads 503 the whole time. Restarting is the operator's call, not this
// binary's.
package main

import (
	"context"
	"errors"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/cafaye/pantry/internal/catalog"
	"github.com/cafaye/pantry/internal/httpapi"
	"github.com/cafaye/pantry/internal/pantrydb"
)

const (
	defaultBind      = "0.0.0.0:8080"
	defaultLogLevel  = "info"
	readHeaderTimout = 5 * time.Second
	// A read timeout and an idle timeout, because a registry that is asked to
	// hold a connection open forever is one client away from being a resource
	// leak. There is no write timeout: a write timeout fires on a slow CLIENT and
	// turns a slow reader into a truncated response, which is worse than a slow
	// reader. Shutdown is bounded by shutdownGrace instead.
	readTimeout   = 15 * time.Second
	idleTimeout   = 60 * time.Second
	shutdownGrace = 10 * time.Second
)

func main() {
	if err := run(); err != nil {
		slog.Error("pantry: exiting", "error", err)
		os.Exit(1)
	}
}

func run() error {
	logger := slog.New(slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{Level: logLevel()}))
	slog.SetDefault(logger)

	bind := envOr("PANTRY_BIND", defaultBind)

	cat, closeStore, detail := mountCatalog(logger)
	// The pool outlives `mountCatalog`, so the CLOSE is handed back rather than
	// deferred inside it. The first version deferred it, which closed the pool the
	// instant the function returned and left every request answering 503 with
	// `closed pool` — found by running the binary against a real database, which is
	// the only way that class of mistake shows up at all: a mocked pool is not a
	// pool with a lifetime.
	defer closeStore()
	logger.Info("pantry: starting",
		"bind", bind,
		"catalog", map[bool]string{true: "postgres", false: "none"}[cat != nil],
		"detail", detail)

	srv := &http.Server{
		Addr: bind,
		Handler: httpapi.New(cat,
			httpapi.WithLogger(logger),
			httpapi.WithUnavailableDetail(detail)),
		ReadHeaderTimeout: readHeaderTimout,
		ReadTimeout:       readTimeout,
		IdleTimeout:       idleTimeout,
	}

	// SIGINT and SIGTERM, not a signal handler per signal: a container runtime
	// sends SIGTERM and a developer sends SIGINT, and both mean "finish what you
	// are doing and go".
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer stop()

	errs := make(chan error, 1)
	go func() {
		logger.Info("pantry: listening", "addr", bind)
		errs <- srv.ListenAndServe()
	}()

	select {
	case err := <-errs:
		if errors.Is(err, http.ErrServerClosed) {
			return nil
		}
		return err
	case <-ctx.Done():
		stop() // a second signal now kills the process rather than being swallowed
		logger.Info("pantry: shutting down", "grace", shutdownGrace.String())
		shutdownCtx, cancel := context.WithTimeout(context.Background(), shutdownGrace)
		defer cancel()
		if err := srv.Shutdown(shutdownCtx); err != nil {
			// The grace is spent, so in-flight requests are dropped. Say so at
			// error level: a silent truncation here is how a deploy eats a
			// request without anybody finding out.
			logger.Error("pantry: shutdown did not finish inside the grace", "error", err)
			return err
		}
		return nil
	}
}

// mountCatalog connects to the registry's database and returns the read path,
// or nil and the sentence every 503 will carry.
//
// IT DOES NOT RETURN AN ERROR, and that is the decision the package comment
// argues: the two things that can go wrong here are a deployment that forgot a
// variable and an outage, and neither of them should crash-loop this process.
// Both are logged, both are named in the `detail`, and `/readyz` reports
// `unavailable` for as long as either is true.
//
// The second return value is the teardown for whatever was mounted, and it is
// always non-nil so the caller can defer it without a check. It DOES log the role
// it took, once, at startup. `internal/pantrydb` refuses a
// connection whose role did not take, so this line cannot be a surprise — but a
// deployment that has quietly changed which group the login role belongs to would
// otherwise be invisible until somebody asked why the catalog is empty, and the
// log line is the cheapest place to answer that.
func mountCatalog(logger *slog.Logger) (catalog.Catalog, func(), string) {
	url := envOr("PANTRY_DATABASE_URL", "")
	if url == "" {
		logger.Error("pantry: no database is configured, so the registry cannot be served",
			"variable", "PANTRY_DATABASE_URL",
			"effect", "GET /v1/services and GET /v1/services/{name} answer 503; /healthz answers 200")
		return nil, func() {}, "no database is configured: PANTRY_DATABASE_URL is unset, so there " +
			"is no registry to read from and nothing to answer with"
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	store, err := pantrydb.Open(ctx, url)
	if err != nil {
		// The URL is not in the message and neither is the driver's own, because a
		// driver's error can quote the URL it was handed and a URL carries a
		// password. `pantrydb.Open` already redacts; this adds nothing so the log
		// cannot become the leak.
		logger.Error("pantry: the registry database could not be reached at startup",
			"error", err,
			"effect", "GET /v1/services and GET /v1/services/{name} answer 503; /healthz answers 200",
			"note", "this is not retried: a retry against a database that is already failing "+
				"multiplies the load")
		return nil, func() {}, "the registry database could not be reached at startup, so there " +
			"is nothing to answer with: " + err.Error()
	}
	role, err := store.VerifyRole(ctx)
	if err != nil {
		// Unreachable in practice — `Open` already refuses a pool whose role did not
		// take — and refused rather than logged because a pool that cannot prove
		// its role is exactly the pool whose empty answer would be indistinguishable
		// from an empty catalog.
		store.Close()
		logger.Error("pantry: the database connection is not using the role the catalog reads as",
			"error", err, "want", pantrydb.PublicRole,
			"effect", "the data routes answer 503 rather than publish an empty catalog")
		return nil, func() {}, "the database connection is not using the " + pantrydb.PublicRole +
			" role, so what it can see is not the public catalog: " + err.Error()
	}

	logger.Info("pantry: the registry catalog is mounted",
		"role", role,
		"note", "tenant isolation and visibility are RLS policies, not query predicates; "+
			"see internal/pantrydb and migrations/00006_rls.sql")
	// `store.Close` as the teardown, so the pool is released on the way out of `run`
	// rather than by a defer this function cannot own.
	return catalog.New(store.Queries()), store.Close, ""
}

func envOr(key, fallback string) string {
	if v, ok := os.LookupEnv(key); ok && v != "" {
		return v
	}
	return fallback
}

// logLevel reads slog's level from a variable and says what it did with a value
// it could not read, rather than silently defaulting — a typo in
// PANTRY_LOG_LEVEL should be visible in the log it was supposed to change.
func logLevel() slog.Level {
	raw := envOr("PANTRY_LOG_LEVEL", defaultLogLevel)
	var level slog.Level
	if err := level.UnmarshalText([]byte(raw)); err != nil {
		slog.Warn("pantry: PANTRY_LOG_LEVEL is not a level this build knows; using info",
			"value", raw, "error", err)
		return slog.LevelInfo
	}
	return level
}
