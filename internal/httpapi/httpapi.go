// Package httpapi wires pantry's HTTP surface.
//
// # THE ROUTING TABLE IS THE DOCUMENT
//
// Every route is registered by oapi-codegen from openapi/v1.yaml
// (`internal/api`), not by hand in this file. A path added here without a path
// in the document would not be served at all, and a path added to the document
// without an implementation here would not compile — the generated
// `api.ServerInterface` is what `Service` satisfies, so a new operationId is a
// compile error rather than a 404 nobody noticed. `routes_test.go` holds the
// remaining direction, the one the compiler cannot: every path in the document
// is mounted.
//
// # WHAT IS ANSWERED, AND WHAT IS NOT
//
// This build mounts NO data source. `internal/catalog`'s Catalog is the seam
// pantry-02 fills with the Postgres-backed read path, and until it is mounted
// the data routes answer the 503 the document already declares for exactly this
// state: "The registry did not load, so there is nothing to answer with. A 200
// with an empty data would be indistinguishable from a platform with no
// services."
//
// That is why `/readyz` answers 503 here rather than `{"status":"ok",
// "services":0}`, and the difference is the whole point of having two probes.
// `/healthz` answers 200 whenever the process is running — the document says so
// in its own words, and an orchestrator that restarted on this would turn a
// missing data source into a crash loop. `/readyz` answers 200 only when there
// is something to serve, so it is the one that says `unavailable` today.
//
// The Rust implementation in `src/` still serves the whole surface from
// `registry/`; this package is the rewrite standing beside it, and pantry-02
// replaces the 503s with the real read path.
package httpapi

import (
	"context"
	"log/slog"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"

	"github.com/cafaye/pantry/internal/api"
	"github.com/cafaye/pantry/internal/catalog"
)

// Service is pantry's implementation of the document's server interface.
type Service struct {
	// catalog is the data source, and nil means none is mounted. It is nil in
	// this build rather than an interface holding a nil pointer, because the
	// difference between "no catalog" and "a catalog that is broken" is the
	// difference the 503 detail has to name, and a nil-pointer interface hides
	// the first behind the second.
	catalog catalog.Catalog
	logger  *slog.Logger
	// now is the clock, injected so a test can age nothing and assert on
	// nothing time-dependent instead of sleeping.
	now func() time.Time
}

type options struct {
	logger *slog.Logger
	now    func() time.Time
}

// Option customises the service.
type Option func(*options)

// WithLogger sets the logger used to report a probe failure.
func WithLogger(logger *slog.Logger) Option {
	return func(o *options) {
		if logger != nil {
			o.logger = logger
		}
	}
}

// WithClock sets the instant readiness reports. It exists because the alternative
// — time.Now() at the call site — is a test seam nobody can reach without a sleep.
func WithClock(now func() time.Time) Option {
	return func(o *options) {
		if now != nil {
			o.now = now
		}
	}
}

// New builds the HTTP handler.
//
// The catalog argument may be nil, and in this build it always is. That is a
// decision the 503 responses name rather than hide: see the package comment.
func New(cat catalog.Catalog, opts ...Option) http.Handler {
	o := options{logger: slog.Default(), now: time.Now}
	for _, opt := range opts {
		opt(&o)
	}
	svc := &Service{catalog: cat, logger: o.logger, now: o.now}

	r := chi.NewRouter()
	// The document says every non-2xx response is a problem, INCLUDING the
	// framework's own 404 and 405. chi's defaults are text/plain, so these two
	// handlers are not decoration: without them the contract holds only for the
	// routes this service writes by hand, and a client that parses problem
	// documents would get HTML from the two responses it is most likely to hit.
	r.NotFound(func(w http.ResponseWriter, r *http.Request) {
		writeProblem(w, r, api.NotFound, http.StatusNotFound,
			"this address is not in openapi/v1.yaml")
	})
	r.MethodNotAllowed(func(w http.ResponseWriter, r *http.Request) {
		writeProblem(w, r, api.MethodNotAllowed, http.StatusMethodNotAllowed,
			"this resource is read-only: the document declares GET and nothing else")
	})
	r.Use(middleware.Recoverer)
	r.Use(traceMiddleware)

	api.HandlerWithOptions(svc, api.ChiServerOptions{
		BaseRouter: r,
		// The generator's default writes `http.Error`, which is text/plain and
		// would be the one non-problem body in the service. It fires on a
		// malformed query parameter — `?limit=abc` — so it is reachable by a
		// client, not only by a malformed request.
		ErrorHandlerFunc: func(w http.ResponseWriter, r *http.Request, err error) {
			writeProblem(w, r, api.ValidationFailed, http.StatusBadRequest,
				paramDetail(err))
		},
	})
	return r
}

// Healthz answers 200 whenever the process is running, including with no
// catalog mounted. The document states that rule and the reason: an
// orchestrator that restarts on this would turn a data problem into a crash
// loop. What the process cannot do is answer a request, and a process that
// cannot bind has never got here.
func (s *Service) Healthz(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, r, http.StatusOK, api.Health{Status: api.HealthStatus("ok")})
}

// Readyz answers 200 only when a catalog is mounted, and 503 naming what could
// not be read when one is not.
//
// It reports the document's `Readiness` shape rather than a list of check
// names, because the document declares this shape and a second, richer body
// would be a second contract.
func (s *Service) Readyz(w http.ResponseWriter, r *http.Request) {
	if s.catalog == nil {
		writeProblem(w, r, api.Unavailable, http.StatusServiceUnavailable, noCatalogDetail)
		return
	}
	count, err := s.catalog.Count(r.Context())
	if err != nil {
		s.logger.ErrorContext(r.Context(), "readiness: the catalog could not be counted",
			"error", err, "trace_id", traceIDFrom(r.Context()))
		writeProblem(w, r, api.Unavailable, http.StatusServiceUnavailable,
			"the catalog is mounted but could not be read: "+err.Error())
		return
	}
	writeJSON(w, r, http.StatusOK, api.Readiness{Status: api.ReadinessStatusOk, Services: count})
}

// ListServices answers the registry, filtered and paged.
//
// Filters, paging and cursors are deliberately NOT implemented in this packet.
// They are the Postgres read path — filtering on kind/language/contract, a
// cursor this service issued — and a filter accepted and ignored is worse than a
// filter refused: it answers 200 with rows the caller did not ask for. So with
// no catalog mounted this is the document's 503, and pantry-02 writes the query.
func (s *Service) ListServices(w http.ResponseWriter, r *http.Request, _ api.ListServicesParams) {
	s.writeNoCatalog(w, r)
}

// GetService answers one registered service, or the document's 404 for an
// unknown name. Same reason as ListServices: with no read path there is nothing
// to look a name up in, and a 200 with an empty object would be a lie a client
// caches.
func (s *Service) GetService(w http.ResponseWriter, r *http.Request, _ string) {
	s.writeNoCatalog(w, r)
}

// noCatalogDetail is ONE sentence, used by every 503 this build writes, because
// the document's own description of the 503 is "the registry did not load, so
// there is nothing to answer with" and a client reads `detail` to learn which of
// the two causes it is. Naming the packet is the whole answer a reader needs:
// the absence is scheduled work, not a misconfiguration to retry.
const noCatalogDetail = "no data source is mounted: pantry-01 stands the rewrite up " +
	"with no read path, and pantry-02 mounts the Postgres-backed one"

func (s *Service) writeNoCatalog(w http.ResponseWriter, r *http.Request) {
	if s.catalog != nil {
		// Unreachable in this build. It is written as a refusal rather than a
		// panic because a future pantry-02 that mounts a catalog must not find
		// this method answering "no catalog" for a mounted one: the panic would
		// say exactly that, in the log, at once.
		panic("catalog mounted but pantry-01 has no read path: pantry-02 replaces this")
	}
	writeProblem(w, r, api.Unavailable, http.StatusServiceUnavailable, noCatalogDetail)
}

// contextKey is unexported so no other package can collide with this one.
type contextKey struct{ name string }

// traceIDContextKey carries the request's trace id.
var traceIDContextKey = contextKey{name: "trace_id"}

func traceIDFrom(ctx context.Context) string {
	id, _ := ctx.Value(traceIDContextKey).(string)
	return id
}

func contextWithTraceID(ctx context.Context, id string) context.Context {
	return context.WithValue(ctx, traceIDContextKey, id)
}
