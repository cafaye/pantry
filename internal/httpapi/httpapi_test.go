package httpapi

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/cafaye/pantry/internal/api"
	"github.com/cafaye/pantry/internal/catalog"
)

// get drives the real router in process. Nothing here opens a socket, so the
// suite has no port to collide on and needs no sleep: the handler runs inside
// this call.
func get(t *testing.T, h http.Handler, method, path string) *httptest.ResponseRecorder {
	t.Helper()
	req := httptest.NewRequest(method, path, nil)
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	return rec
}

// decode reads a body and fails the test naming the status and body when it is
// not the JSON the caller expected — an assertion that prints "expected 200" and
// a 503 body is a test that has to be debugged twice.
func decode[T any](t *testing.T, rec *httptest.ResponseRecorder) T {
	t.Helper()
	var v T
	body, err := io.ReadAll(rec.Body)
	if err != nil {
		t.Fatalf("reading the body: %v", err)
	}
	if err := json.Unmarshal(body, &v); err != nil {
		t.Fatalf("body is not the JSON this test expected (%d %s): %s", rec.Code, rec.Header().Get("Content-Type"), body)
	}
	return v
}

// The document says every 2xx response carries an X-Trace-Id, on the 200s as
// well as on the problems. A trace id that appears only when something went
// wrong is a trace id you cannot follow into the request that caused it.
func TestEveryResponseCarriesATraceId(t *testing.T) {
	for _, path := range []string{"/healthz", "/readyz", "/v1/services", "/v1/services/identity", "/nope"} {
		rec := get(t, New(nil), http.MethodGet, path)
		id := rec.Header().Get("X-Trace-Id")
		if len(id) != 32 {
			t.Errorf("GET %s: X-Trace-Id is %q, want 32 hex characters", path, id)
		}
	}
}

// An inbound trace id is ADOPTED, because that is what makes it useful across a
// hop: guard hands one down and support searches for it in every service that
// answered. An id that is not this service's shape is replaced rather than
// trusted — an unbounded header from the internet is a log-injection vector.
func TestAnInboundTraceIdIsAdoptedAndAnUnboundedOneIsNot(t *testing.T) {
	inbound := "0123456789abcdef0123456789abcdef"
	req := httptest.NewRequest(http.MethodGet, "/healthz", nil)
	req.Header.Set("X-Trace-Id", inbound)
	rec := httptest.NewRecorder()
	New(nil).ServeHTTP(rec, req)
	if got := rec.Header().Get("X-Trace-Id"); got != inbound {
		t.Errorf("X-Trace-Id is %q, want the inbound %q adopted unchanged", got, inbound)
	}

	for _, hostile := range []string{
		strings.Repeat("a", 4096) + "\n\r\nInjected: 1",
		"not-hex",
		"",
	} {
		req := httptest.NewRequest(http.MethodGet, "/healthz", nil)
		req.Header.Set("X-Trace-Id", hostile)
		rec := httptest.NewRecorder()
		New(nil).ServeHTTP(rec, req)
		got := rec.Header().Get("X-Trace-Id")
		if len(got) != 32 {
			t.Errorf("inbound %q: X-Trace-Id is %q, want a minted 32-character id", hostile[:min(len(hostile), 20)], got)
		}
	}
}

// /healthz is 200 with no catalog mounted. The document states that in its own
// words — a process that cannot serve is still alive, and an orchestrator that
// restarts on this turns a data problem into a crash loop.
func TestHealthzIsTwoHundredWithNoCatalogMounted(t *testing.T) {
	rec := get(t, New(nil), http.MethodGet, "/healthz")
	if rec.Code != http.StatusOK {
		t.Fatalf("GET /healthz is %d, want 200", rec.Code)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Errorf("GET /healthz Content-Type is %q, want application/json", got)
	}
	health := decode[api.Health](t, rec)
	if health.Status != api.HealthStatusOk {
		t.Errorf("GET /healthz status is %q, want %q", health.Status, api.HealthStatusOk)
	}
}

// /readyz is 503 with no catalog mounted, and its detail says WHY rather than
// leaving an operator to guess between "the registry is empty" and "the registry
// failed to load" — which are different incidents with different fixes.
//
// The detail is now supplied by whoever mounts the handler rather than being a
// constant, because the cause is a deployment's and the two causes are different
// sentences: PANTRY_DATABASE_URL unset is a manifest mistake, an unreachable
// database is an outage, and a 503 that says the same thing for both sends the
// reader to the wrong one first. A constant here used to name a packet, which
// stopped being true the moment the read path landed.
func TestReadyzIsFiveOhThreeAndNamesTheCause(t *testing.T) {
	const cause = "PANTRY_DATABASE_URL is unset, so there is no registry to read from"
	rec := get(t, New(nil, WithUnavailableDetail(cause)), http.MethodGet, "/readyz")
	if rec.Code != http.StatusServiceUnavailable {
		t.Fatalf("GET /readyz is %d, want 503", rec.Code)
	}
	problem := decode[api.Problem](t, rec)
	if problem.Code != api.Unavailable || !problem.Code.Valid() {
		t.Errorf("code is %q, want a member of the document's reserved list (%q)", problem.Code, api.Unavailable)
	}
	if problem.Status != http.StatusServiceUnavailable {
		t.Errorf("body status is %d, want it to match the response status 503", problem.Status)
	}
	if problem.Instance != "/readyz" {
		t.Errorf("instance is %q, want /readyz", problem.Instance)
	}
	if problem.TraceID != rec.Header().Get("X-Trace-Id") {
		t.Errorf("body trace_id is %q and the header says %q; the document says they are equal",
			problem.TraceID, rec.Header().Get("X-Trace-Id"))
	}
	if problem.Detail != cause {
		t.Errorf("detail is %q, want the cause the mount declared, verbatim: %q", problem.Detail, cause)
	}
	if want := "https://errors.cafaye.com/" + string(api.Unavailable); problem.Type != want {
		t.Errorf("type is %q, want %q", problem.Type, want)
	}
}

// The 200 shape of readiness, driven by a fake catalog, so the probe is written
// once and the assertion is about the CONTRACT rather than about this build's
// absence. Without this the 503 test above would pass on an implementation that
// could never answer 200 at all.
func TestReadyzIsTwoHundredWithACatalogMounted(t *testing.T) {
	rec := get(t, New(fakeCatalog{services: 3}), http.MethodGet, "/readyz")
	if rec.Code != http.StatusOK {
		t.Fatalf("GET /readyz is %d, want 200 with a catalog mounted", rec.Code)
	}
	readiness := decode[api.Readiness](t, rec)
	if readiness.Status != api.ReadinessStatusOk {
		t.Errorf("status is %q, want %q", readiness.Status, api.ReadinessStatusOk)
	}
	if readiness.Services != 3 {
		t.Errorf("services is %d, want the 3 the catalog reported", readiness.Services)
	}
}

// A catalog that is mounted and CANNOT be read is a different incident from no
// catalog, and the 503 says so. Reporting both as "unavailable" with the same
// detail is how a database outage gets filed as a deploy problem.
func TestReadyzDistinguishesAMountedCatalogThatCannotBeRead(t *testing.T) {
	broken := fakeCatalog{err: context.DeadlineExceeded}
	rec := get(t, New(broken), http.MethodGet, "/readyz")
	if rec.Code != http.StatusServiceUnavailable {
		t.Fatalf("GET /readyz is %d, want 503", rec.Code)
	}
	problem := decode[api.Problem](t, rec)
	if strings.Contains(problem.Detail, "could not be read from its database") {
		t.Errorf("detail is %q, want it to name the READ failure and not the missing-mount "+
			"sentence: a catalog that is mounted and cannot be read is a different incident",
			problem.Detail)
	}
	if !strings.Contains(problem.Detail, "deadline exceeded") {
		t.Errorf("detail is %q, want it to carry the underlying cause", problem.Detail)
	}
}

// WITH NO CATALOG MOUNTED, the two data routes answer the document's declared
// 503 and never a 200 with an empty body — an empty `data: []` is
// indistinguishable from a platform with no services, which is the exact confusion
// the document names, and with nothing mounted the service cannot tell the
// difference.
//
// A catalog mounted over a database that HAS ANSWERED is the other case and it is
// a 200 with `data: []`; `TestTheThreeCasesAreDistinguishable` in
// `internal/pantrydb` holds all three against a real PostgreSQL.
func TestTheDataRoutesAreFiveOhThreeWithNothingMounted(t *testing.T) {
	for _, path := range []string{"/v1/services", "/v1/services/identity", "/v1/services?kind=api&limit=25"} {
		rec := get(t, New(nil), http.MethodGet, path)
		if rec.Code != http.StatusServiceUnavailable {
			t.Errorf("GET %s is %d, want 503", path, rec.Code)
			continue
		}
		problem := decode[api.Problem](t, rec)
		if problem.Code != api.Unavailable {
			t.Errorf("GET %s code is %q, want %q", path, problem.Code, api.Unavailable)
		}
		if strings.Contains(rec.Body.String(), `"data"`) {
			t.Errorf("GET %s answered a registry-shaped body: %s", path, rec.Body.String())
		}
	}
}

// RFC 9457 has its own media type and the document names it on every non-2xx.
// A client that switches on Content-Type must not see application/json from an
// error.
func TestProblemResponsesUseTheProblemMediaType(t *testing.T) {
	for _, path := range []string{"/readyz", "/v1/services", "/v1/services/identity"} {
		rec := get(t, New(nil), http.MethodGet, path)
		if got := rec.Header().Get("Content-Type"); got != "application/problem+json" {
			t.Errorf("GET %s Content-Type is %q, want application/problem+json", path, got)
		}
	}
}

// chi's own 404 and 405 are responses this service serves, so they are problem
// documents too. The document says "including the framework's own 404 and 405";
// without these two handlers the service would be correct on every route it
// writes by hand and wrong on the two a client is most likely to hit by mistake.
func TestTheFrameworksOwnNotFoundAndMethodNotAllowedAreProblems(t *testing.T) {
	cases := []struct {
		method, path string
		status       int
		code         api.ProblemCode
	}{
		{http.MethodGet, "/v1/nope", http.StatusNotFound, api.NotFound},
		{http.MethodGet, "/v1/services/identity/extra", http.StatusNotFound, api.NotFound},
		{http.MethodPost, "/v1/services", http.StatusMethodNotAllowed, api.MethodNotAllowed},
		{http.MethodDelete, "/v1/services/identity", http.StatusMethodNotAllowed, api.MethodNotAllowed},
	}
	for _, tc := range cases {
		rec := get(t, New(nil), tc.method, tc.path)
		if rec.Code != tc.status {
			t.Errorf("%s %s is %d, want %d", tc.method, tc.path, rec.Code, tc.status)
			continue
		}
		problem := decode[api.Problem](t, rec)
		if problem.Code != tc.code {
			t.Errorf("%s %s code is %q, want %q", tc.method, tc.path, problem.Code, tc.code)
		}
		if problem.Status != tc.status {
			t.Errorf("%s %s body status is %d, want %d", tc.method, tc.path, problem.Status, tc.status)
		}
		if problem.Instance != tc.path {
			t.Errorf("%s %s instance is %q, want the request path", tc.method, tc.path, problem.Instance)
		}
	}
}

// `?limit=abc` is reachable by any client, and it lands in the generator's
// parameter-binding error, which the default handler would write as text/plain.
// This asserts both that it is a problem document AND that it names the
// parameter — a 400 that says "bad request" without saying which parameter is a
// 400 a caller cannot act on.
func TestAMalformedParameterIsAProblemThatNamesTheParameter(t *testing.T) {
	rec := get(t, New(nil), http.MethodGet, "/v1/services?limit=abc")
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("GET /v1/services?limit=abc is %d, want 400", rec.Code)
	}
	problem := decode[api.Problem](t, rec)
	if problem.Code != api.ValidationFailed {
		t.Errorf("code is %q, want %q", problem.Code, api.ValidationFailed)
	}
	if !strings.Contains(problem.Detail, "limit") {
		t.Errorf("detail is %q, want it to name the parameter", problem.Detail)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/problem+json" {
		t.Errorf("Content-Type is %q, want application/problem+json", got)
	}
}

// An unknown query parameter is a 400 in this contract, and refusing it is what
// stops a client asking a question this version cannot answer and being handed
// the whole list instead. Nothing is mounted here, so the assertion is only that
// it refuses rather than serves; the mounted-catalog version — which has to be a
// 400 and not a 503 — is in `internal/pantrydb`'s three-case test.
func TestAnUnknownQueryParameterIsRefusedRatherThanIgnored(t *testing.T) {
	rec := get(t, New(nil), http.MethodGet, "/v1/services?sort=name")
	if rec.Code == http.StatusOK {
		t.Fatalf("GET /v1/services?sort=name is 200; an unknown parameter must not be silently ignored")
	}
}

// The service carries no write verb. This is pantry's rule (tests/scoping.rs
// asserts it for the Rust service) and the Go service inherits it: the document
// declares four GETs and nothing else, so every other verb on those paths is a
// 405.
func TestNoWriteVerbIsAcceptedOnAnyPath(t *testing.T) {
	for _, method := range []string{http.MethodPost, http.MethodPut, http.MethodPatch, http.MethodDelete} {
		for _, path := range []string{"/v1/services", "/v1/services/identity", "/healthz", "/readyz"} {
			rec := get(t, New(nil), method, path)
			if rec.Code != http.StatusMethodNotAllowed {
				t.Errorf("%s %s is %d, want 405", method, path, rec.Code)
			}
		}
	}
}

// A panic in a handler must not become a crash, and must not become a
// text/plain 500 either: the document's promise covers every non-2xx this
// service writes, and a panic is one.
func TestAPanicBecomesAProblemAndNotACrash(t *testing.T) {
	panicky := New(panicOnCount{})
	rec := get(t, panicky, http.MethodGet, "/readyz")
	if rec.Code != http.StatusInternalServerError {
		t.Fatalf("GET /readyz with a panicking catalog is %d, want 500", rec.Code)
	}
	problem := decode[api.Problem](t, rec)
	if problem.Code != api.Internal || !problem.Code.Valid() {
		t.Errorf("code is %q, want a member of the document's reserved list", problem.Code)
	}
	if problem.TraceID == "" {
		t.Error("trace_id is empty; a 500 is exactly the response support starts from")
	}
}

type fakeCatalog struct {
	services int
	err      error
}

func (f fakeCatalog) List(context.Context, catalog.Filter) ([]api.Service, *string, error) {
	return nil, nil, f.err
}

func (f fakeCatalog) Get(context.Context, string) (api.Service, error) {
	return api.Service{}, catalog.ErrNoSuchService
}

func (f fakeCatalog) Count(context.Context) (int, error) {
	if f.err != nil {
		return 0, f.err
	}
	return f.services, nil
}

func (f fakeCatalog) Requirements(context.Context, string) ([]api.CompatibilityEdge, error) {
	return nil, f.err
}

func (f fakeCatalog) RequiredBy(context.Context, string) ([]api.CompatibilityEdge, error) {
	return nil, f.err
}

// panicOnCount is the fault injection for the panic path, and it replaces the
// CATALOG rather than the router: the request still passes through chi, still
// gets a trace id, still recovers — the whole chain production uses.
type panicOnCount struct{}

func (panicOnCount) List(context.Context, catalog.Filter) ([]api.Service, *string, error) {
	return nil, nil, nil
}
func (panicOnCount) Get(context.Context, string) (api.Service, error) { return api.Service{}, nil }
func (panicOnCount) Count(context.Context) (int, error)               { panic("injected") }
func (panicOnCount) Requirements(context.Context, string) ([]api.CompatibilityEdge, error) {
	return nil, nil
}
func (panicOnCount) RequiredBy(context.Context, string) ([]api.CompatibilityEdge, error) {
	return nil, nil
}

// graphCatalog is the fake for the two graph operations, and it keeps the two
// directions' answers in separate fields because the whole point of the tests
// below is that the two questions are not the same question read from two ends.
//
// `known` is the set of names the fake answers with edges; anything else is
// ErrNoSuchService, which is what makes the 404 path testable from the handler
// down without a database.
type graphCatalog struct {
	requirements map[string][]api.CompatibilityEdge
	requiredBy   map[string][]api.CompatibilityEdge
	known        []string
	err          error
}

func (g graphCatalog) List(context.Context, catalog.Filter) ([]api.Service, *string, error) {
	return nil, nil, g.err
}
func (g graphCatalog) Get(_ context.Context, name string) (api.Service, error) {
	for _, k := range g.known {
		if k == name {
			return api.Service{Name: name}, nil
		}
	}
	return api.Service{}, catalog.ErrNoSuchService
}
func (g graphCatalog) Count(context.Context) (int, error) { return len(g.known), nil }

// Both graph methods answer ErrNoSuchService for a name outside `known`, on
// PURPOSE and in the fake rather than only in `Postgres`: the interface's
// contract is that an unknown subject is an error, and if the fake answered a
// quiet empty list where the real catalog answers a sentinel, these handler
// tests would be certifying a 200 that no real database can produce. A fake
// that is more forgiving than the thing it stands in for is a fake that hides
// the exact bug the 404 test below exists for.
func (g graphCatalog) Requirements(_ context.Context, name string) ([]api.CompatibilityEdge, error) {
	if g.err != nil {
		return nil, g.err
	}
	if !g.knows(name) {
		return nil, catalog.ErrNoSuchService
	}
	edges := g.requirements[name]
	if edges == nil {
		edges = []api.CompatibilityEdge{}
	}
	return edges, nil
}
func (g graphCatalog) RequiredBy(_ context.Context, name string) ([]api.CompatibilityEdge, error) {
	if g.err != nil {
		return nil, g.err
	}
	if !g.knows(name) {
		return nil, catalog.ErrNoSuchService
	}
	edges := g.requiredBy[name]
	if edges == nil {
		edges = []api.CompatibilityEdge{}
	}
	return edges, nil
}

func (g graphCatalog) knows(name string) bool {
	for _, k := range g.known {
		if k == name {
			return true
		}
	}
	return false
}

// The seed's graph, so the fixtures below read like the database the fake
// stands in for: muse requires identity and courier; identity is required by
// courier and muse.
var (
	museRequirements = []api.CompatibilityEdge{
		{Name: "courier", VersionRange: "^0.1.0", Dependency: api.Required},
		{Name: "identity", VersionRange: "^0.1.0", Dependency: api.Required},
	}
	identityRequiredBy = []api.CompatibilityEdge{
		{Name: "courier", VersionRange: "^0.1.0", Dependency: api.Required},
		{Name: "muse", VersionRange: "^0.1.0", Dependency: api.Required},
	}
)

// TestRequirementsIsTheForwardDirectionOfTheGraph asserts the 200 path, and
// three things on it that are easy to get wrong separately:
//
//   - the `service` field echoes the path parameter, because a client that
//     holds several responses must be able to tell them apart without having
//     kept the request URLs;
//   - the edges are the CATALOG's edges, serialised through the document's
//     generated type — a pantry-owned edge shape here would be a second
//     description of the same row and they would drift;
//   - the three fields are all present, because `dependency` is the field that
//     separates "does not start without it" from "runs degraded", and an edge
//     that loses it turns a warning into an outage.
func TestRequirementsIsTheForwardDirectionOfTheGraph(t *testing.T) {
	cat := graphCatalog{
		requirements: map[string][]api.CompatibilityEdge{"muse": museRequirements},
		known:        []string{"muse"},
	}
	rec := get(t, New(cat), http.MethodGet, "/v1/services/muse/requirements")
	if rec.Code != http.StatusOK {
		t.Fatalf("GET requirements is %d, want 200", rec.Code)
	}
	body := decode[api.ServiceRequirements](t, rec)
	if body.Service != "muse" {
		t.Errorf("service is %q, want the path parameter echoed", body.Service)
	}
	if len(body.Data) != 2 {
		t.Fatalf("data holds %d edges, want 2", len(body.Data))
	}
	if body.Data[0].Name != "courier" || body.Data[1].Name != "identity" {
		t.Errorf("edges are [%s %s], want ordered by the required service's name",
			body.Data[0].Name, body.Data[1].Name)
	}
	if body.Data[0].VersionRange != "^0.1.0" || body.Data[0].Dependency != api.Required {
		t.Errorf("edge is %+v, want the publisher's range verbatim and the dependency named",
			body.Data[0])
	}
}

// TestRequiredByIsTheOtherQuestionAndNotTheSameAnswer runs ONE catalog through
// BOTH operations for the same name and asserts the answers differ, because the
// failure this guards against is not a wrong query — it is the day somebody
// "simplifies" the two handlers into one that answers `Requirements` for both
// paths. Every response would still be a 200 with plausible edges in it, and
// the backward answer would be about the wrong endpoint.
//
// The wrappers being distinct generated types is the compile-time half of that
// guard; this is the runtime half, because a shared handler could satisfy both
// signatures with one type.
func TestRequiredByIsTheOtherQuestionAndNotTheSameAnswer(t *testing.T) {
	// `identity` is the subject the seed actually supports for this: the graph
	// holds no edge WITH identity as its source, so its forward answer is the
	// empty list while its backward answer holds two services. Both halves of
	// the assertion are then about the same subject, which is the comparison
	// that catches one handler answering the other's question.
	cat := graphCatalog{
		requirements: map[string][]api.CompatibilityEdge{"identity": {}},
		requiredBy:   map[string][]api.CompatibilityEdge{"identity": identityRequiredBy},
		known:        []string{"identity"},
	}
	fwd := get(t, New(cat), http.MethodGet, "/v1/services/identity/requirements")
	bwd := get(t, New(cat), http.MethodGet, "/v1/services/identity/required-by")
	if fwd.Code != http.StatusOK || bwd.Code != http.StatusOK {
		t.Fatalf("GET requirements is %d and GET required-by is %d, want 200 and 200",
			fwd.Code, bwd.Code)
	}
	fwdBody := decode[api.ServiceRequirements](t, fwd)
	bwdBody := decode[api.ServiceRequiredBy](t, bwd)
	if len(fwdBody.Data) != 0 {
		t.Errorf("identity requires %d edges through the fake, want none — the fixture wants "+
			"a subject whose two answers CANNOT be confused", len(fwdBody.Data))
	}
	if len(bwdBody.Data) != 2 {
		t.Errorf("required-by(identity) holds %d edges, want the two services the seed says "+
			"require it", len(bwdBody.Data))
	}
	if bwdBody.Service != "identity" {
		t.Errorf("required-by's service field is %q, want the SUBJECT echoed — the service "+
			"being depended on, not the services that depend on it", bwdBody.Service)
	}
}

// TestTheGraphRoutesAnswer404ForAnUnknownName — the document's rule for the
// collection ("no service matches" is a 200 with an empty list) does NOT extend
// to the graph routes, because there is no collection here: the path names one
// service, and a service that does not exist makes the question unanswerable
// rather than empty. A 200 with `data: []` on a name that is not registered
// would tell a client that a service with no dependencies exists, and they
// would go and try to run it.
func TestTheGraphRoutesAnswer404ForAnUnknownName(t *testing.T) {
	for _, path := range []string{"/v1/services/nope/requirements", "/v1/services/nope/required-by"} {
		rec := get(t, New(graphCatalog{known: []string{"muse"}}), http.MethodGet, path)
		if rec.Code != http.StatusNotFound {
			t.Errorf("GET %s is %d, want 404", path, rec.Code)
			continue
		}
		problem := decode[api.Problem](t, rec)
		if problem.Code != api.NotFound {
			t.Errorf("GET %s code is %q, want %q", path, problem.Code, api.NotFound)
		}
	}
}

// TestALeafOfTheGraphIsAnAnswerNotAnAbsence — the other 200, and the one the
// 404 test above exists to be distinct from. `caf` requires nothing and is
// required by nothing; both of its answers are empty ARRAYS and not nulls,
// because the document's convention is "empty rather than absent" and a JSON
// null in `data` makes every client's loop a special case.
func TestALeafOfTheGraphIsAnAnswerNotAnAbsence(t *testing.T) {
	cat := graphCatalog{requirements: map[string][]api.CompatibilityEdge{},
		requiredBy: map[string][]api.CompatibilityEdge{}, known: []string{"caf"}}
	for _, tc := range []struct {
		path string
		any  func() error
	}{
		{"/v1/services/caf/requirements", func() error {
			b := decode[api.ServiceRequirements](t,
				get(t, New(cat), http.MethodGet, "/v1/services/caf/requirements"))
			if b.Data == nil {
				return errors.New("data is null, want []")
			}
			if len(b.Data) != 0 {
				return fmt.Errorf("data holds %d edges, want none", len(b.Data))
			}
			return nil
		}},
		{"/v1/services/caf/required-by", func() error {
			b := decode[api.ServiceRequiredBy](t,
				get(t, New(cat), http.MethodGet, "/v1/services/caf/required-by"))
			if b.Data == nil {
				return errors.New("data is null, want []")
			}
			if len(b.Data) != 0 {
				return fmt.Errorf("data holds %d edges, want none", len(b.Data))
			}
			return nil
		}},
	} {
		if err := tc.any(); err != nil {
			t.Errorf("GET %s: %v", tc.path, err)
		}
	}
}
