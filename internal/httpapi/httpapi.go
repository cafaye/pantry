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
// # THREE ANSWERS, AND THEY MUST NOT LOOK ALIKE
//
// This is the whole reason the data routes used to answer 503, and it is now the
// reason they mostly do not. `GET /v1/services` has three possible truths and a
// client must be able to tell them apart without reading prose:
//
//	rows are there             200 with `data: [ … ]`
//	the catalog is empty       200 with `data: []`     — the database ANSWERED
//	the database is not there  503 with a problem      — the database did not
//
// The middle one used to be impossible to express, because a 200 with `services:
// 0` is indistinguishable from a platform that has no services, which is the
// confusion the 503 was built to prevent. It is a real answer now, and it is
// different precisely because the query ran.
//
// So there is NO fallback anywhere in this file. A query that fails is a 503, not
// an empty page; an empty page is a 200, not a warning; and there is no retry — a
// retry against a database that is already failing multiplies the load.
//
// `/readyz` is the other half and it must agree. `/healthz` answers 200 whenever
// the process is running, including with no data source mounted, because the
// document says so and an orchestrator that restarted on it would turn a missing
// data source into a crash loop. `/readyz` answers 200 only when there is
// something to serve.
//
// # WHERE THE FILTERS ARE CHECKED, AND WHY IT IS NOT HERE
//
// The generated binder reads `?kind=`, `?language=`, `?contract=`, `?limit=` and
// `?cursor=` as strings. It does NOT check them against the document's enums, does
// NOT apply `limit`'s default, and does NOT refuse `limit=0`. Those checks are real
// work with nowhere to live except somewhere this package chooses, and they live in
// `catalog.Filter.Validate` — next to the query that obeys them, where a handler
// cannot forget them. What stays here is the one thing only a handler can know:
// whether a parameter the caller sent is a parameter this document declares.
//
// The Rust implementation in `src/` still serves the whole surface from
// `registry/`; this package is the rewrite standing beside it.
package httpapi

import (
	"context"
	"errors"
	"log/slog"
	"net/http"
	"sort"
	"strings"
	"time"

	"github.com/go-chi/chi/v5"

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
	// unavailableDetail names WHY no catalog is mounted, and it is a field rather
	// than a constant because the cause is a deployment's: an unset
	// PANTRY_DATABASE_URL and an unreachable one are different sentences, and one
	// sentence cannot be right for both. The previous packet's constant named a
	// packet, which stopped being true the moment the read path landed.
	unavailableDetail string
}

type options struct {
	logger            *slog.Logger
	now               func() time.Time
	unavailableDetail string
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

// WithUnavailableDetail sets the `detail` of every 503 this service writes while
// no catalog is mounted.
func WithUnavailableDetail(detail string) Option {
	return func(o *options) {
		if detail != "" {
			o.unavailableDetail = detail
		}
	}
}

// New builds the HTTP handler.
//
// The catalog argument MAY be nil, and there are two ways to be in that state
// which are both legitimate rather than both being failures: a binary started
// with no PANTRY_DATABASE_URL, and a binary whose database could not be reached at
// boot. Both answer 503 on the data routes and 200 on `/healthz`, and the
// difference between them is a sentence.
func New(cat catalog.Catalog, opts ...Option) http.Handler {
	o := options{logger: slog.Default(), now: time.Now}
	for _, opt := range opts {
		opt(&o)
	}
	svc := &Service{
		catalog:           cat,
		logger:            o.logger,
		now:               o.now,
		unavailableDetail: o.unavailableDetail,
	}

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
	r.Use(traceMiddleware)
	r.Use(recoverMiddleware)

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
		// The same helper the data routes use, so `/readyz` and
		// `GET /v1/services` cannot disagree about WHY there is nothing to serve.
		// They used to: this branch carried its own copy of the detail.
		s.writeNoCatalog(w, r)
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
// The three failure modes are three DIFFERENT answers and this function is where
// that is decided:
//
//   - no catalog mounted          503, and the detail says why it is not mounted
//   - a parameter this document cannot answer, or a cursor this service did not
//     issue                     400, naming the parameter and what was allowed
//   - the query itself failed    503, with the driver's own words in the log
//     and a sentence in the body. NEVER an empty
//     page: an empty page is a 200, and a 200 that
//     means "the database is down" is the exact lie
//     this packet exists to stop telling.
//
// A filter that matches nothing is the fourth case and it is a 200 with `data: []`
// — the fourth element of `ServiceList` is REQUIRED and `data` "is always an
// array, empty rather than absent", so this is the only body that can express it.
func (s *Service) ListServices(w http.ResponseWriter, r *http.Request, params api.ListServicesParams) {
	if s.catalog == nil {
		s.writeNoCatalog(w, r)
		return
	}
	if detail := unknownQueryParameter(r, listQueryParameters); detail != "" {
		writeProblem(w, r, api.ValidationFailed, http.StatusBadRequest, detail)
		return
	}

	services, next, err := s.catalog.List(r.Context(), catalog.Filter{
		Kind:     params.Kind,
		Language: params.Language,
		Contract: params.Contract,
		Limit:    params.Limit,
		Cursor:   params.Cursor,
	})
	switch {
	case errors.Is(err, catalog.ErrBadFilter):
		writeProblem(w, r, api.ValidationFailed, http.StatusBadRequest, err.Error())
		return
	case err != nil:
		s.logger.ErrorContext(r.Context(), "catalog: the registry could not be read",
			"error", err, "trace_id", traceIDFrom(r.Context()))
		writeProblem(w, r, api.Unavailable, http.StatusServiceUnavailable,
			"the registry could not be read from its database: "+err.Error())
		return
	}

	if services == nil {
		// `data` is REQUIRED and "always an array, empty rather than absent". A nil
		// slice marshals as `null`, which the document does not publish for this
		// key and which a client written against it cannot tell from a different
		// shape entirely. Every implementation of `Catalog` owes the handler this
		// much, and doing it here rather than in each one is the point.
		services = []api.Service{}
	}
	writeJSON(w, r, http.StatusOK, api.ServiceList{
		Data: services,
		Page: api.Page{NextCursor: next, HasMore: next != nil},
	})
}

// GetService answers one registered service, or the document's 404 for an
// unknown name.
//
// The 404 covers a name that does not exist AND a name whose row exists but is
// not visible to this role, and they are deliberately the same answer: to a
// public reader a draft is not a service that exists, and telling the difference
// would be telling a caller what another publisher has not published. RLS draws
// the line underneath the query rather than a predicate in it.
func (s *Service) GetService(w http.ResponseWriter, r *http.Request, name string) {
	if s.catalog == nil {
		s.writeNoCatalog(w, r)
		return
	}

	svc, err := s.catalog.Get(r.Context(), name)
	switch {
	case errors.Is(err, catalog.ErrNoSuchService):
		writeProblem(w, r, api.NotFound, http.StatusNotFound,
			"no official cafaye service is registered under that name")
		return
	case err != nil:
		s.logger.ErrorContext(r.Context(), "catalog: one service could not be read",
			"name", name, "error", err, "trace_id", traceIDFrom(r.Context()))
		writeProblem(w, r, api.Unavailable, http.StatusServiceUnavailable,
			"the registry could not be read from its database: "+err.Error())
		return
	}
	writeJSON(w, r, http.StatusOK, svc)
}

// listQueryParameters is the document's `GET /v1/services` parameter set, and it
// is written out here because `openapi/v1.yaml` is not readable at runtime.
//
// It is derived from the document by reading it, not by reading the handler: the
// five names are the document's, and a sixth parameter added to the document
// without being added here is caught by `TestTheParameterListMatchesTheDocument`,
// which counts the names a client can send against the names this list holds.
var listQueryParameters = []string{"kind", "language", "contract", "limit", "cursor"}

// unknownQueryParameter refuses a parameter the document does not declare.
//
// "An unknown parameter is also a 400, so a client that asks a question this
// version cannot answer is told rather than quietly given the whole list." That
// sentence exists because the alternative is worse than being unhelpful: a client
// sending `?kinds=api` gets the entire registry and concludes every entry is of
// the requested kind, and nothing in the response says otherwise.
//
// It is a handler's job rather than the catalog's because only a handler can see
// the raw query string — by the time a `Filter` exists, a misspelled parameter has
// already been dropped.
func unknownQueryParameter(r *http.Request, declared []string) string {
	known := make(map[string]struct{}, len(declared))
	for _, name := range declared {
		known[name] = struct{}{}
	}
	var unknown []string
	for name := range r.URL.Query() {
		if _, ok := known[name]; !ok {
			unknown = append(unknown, name)
		}
	}
	if len(unknown) == 0 {
		return ""
	}
	sort.Strings(unknown)
	allowed := make([]string, len(declared))
	copy(allowed, declared)
	sort.Strings(allowed)
	return "query parameter(s) " + strings.Join(unknown, ", ") +
		" are not declared by this version of the document. declared: " +
		strings.Join(allowed, ", ")
}

// noCatalogDetail is the fallback for a `New` that was given no detail, which
// only happens when a test mounts the handler without saying why. It says the
// thing that is true in every such case and nothing that is not.
const noCatalogDetail = "no data source is mounted: this service has no catalog to read from, " +
	"so there is nothing to answer with"

func (s *Service) writeNoCatalog(w http.ResponseWriter, r *http.Request) {
	if s.catalog != nil {
		// Unreachable, and written as a refusal rather than a panic because a
		// handler that reached here with a catalog mounted would be answering
		// "there is nothing to serve" about a registry that has rows in it. The
		// panic says exactly that, in the log, at once.
		panic("httpapi: a catalog is mounted but this handler reported no data source; " +
			"a mounted catalog must never answer with the 503")
	}
	detail := s.unavailableDetail
	if detail == "" {
		detail = noCatalogDetail
	}
	writeProblem(w, r, api.Unavailable, http.StatusServiceUnavailable, detail)
}

// recoverMiddleware turns a panic into the document's 500 problem rather than
// chi's text/plain one.
//
// chi's own Recoverer would write "panic recovered\n" with a text/plain content
// type, which breaks the promise the document makes about every non-2xx this
// service writes — and a 500 is the one response a client is guaranteed to see
// eventually, so it is the worst one to get wrong. The cause is logged with the
// request's trace id, because a 500 whose detail says "internal error" and whose
// log line cannot be found is an incident nobody closes.
func recoverMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		defer func() {
			cause := recover()
			if cause == nil {
				return
			}
			if cause == http.ErrAbortHandler {
				// The documented way to abandon a response. Not an incident, and
				// re-panicking would make the net/http server log it as one.
				panic(cause)
			}
			slog.Default().ErrorContext(r.Context(), "httpapi: recovered from a panic",
				"method", r.Method, "path", r.URL.Path,
				"trace_id", traceIDFrom(r.Context()), "panic", cause)
			writeProblem(w, r, api.Internal, http.StatusInternalServerError,
				"the request could not be served: an internal error occurred")
		}()
		next.ServeHTTP(w, r)
	})
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
