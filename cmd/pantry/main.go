// Command pantry serves the cafaye service registry.
//
// This is the Go rewrite, standing beside the Rust implementation in `src/` —
// see the packet: pantry-01 stands the service up, pantry-02 gives it a data
// source. The two are never both serving: the Rust image is what a deployment
// runs until the Go one has a read path, and this binary answers the document's
// 503 on the data routes until then.
//
// # CONFIGURATION, and the two variables that are the service's whole surface
//
//	PANTRY_BIND       listen address, default 0.0.0.0:8080
//	PANTRY_LOG_LEVEL  slog level, default info
//
// There is no database URL, no registry directory and no flag set, and the
// absence is the point: a variable this build ignores is a lie in a deployment
// manifest. pantry-02 adds the connection settings when there is a connection
// to make.
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

	"github.com/cafaye/pantry/internal/httpapi"
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

	// No catalog is mounted. This build answers 503 on the data routes and says
	// so in the log line, rather than serving an empty registry that a client
	// would cache as "this platform has no services".
	logger.Info("pantry: starting",
		"bind", bind,
		"catalog", "none",
		"note", "pantry-02 mounts the read path; /readyz reports unavailable until then")

	srv := &http.Server{
		Addr:              bind,
		Handler:           httpapi.New(nil, httpapi.WithLogger(logger)),
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
