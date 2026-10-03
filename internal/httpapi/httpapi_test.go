package httpapi

import (
	"context"
	"encoding/json"
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

// /readyz is 503 until a catalog is mounted, and its detail says WHY rather than
// leaving an operator to guess between "the registry is empty" and "the registry
// failed to load" — which are different incidents with different fixes.
func TestReadyzIsFiveOhThreeAndNamesTheCause(t *testing.T) {
	rec := get(t, New(nil), http.MethodGet, "/readyz")
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
	if !strings.Contains(problem.Detail, "pantry-02") {
		t.Errorf("detail is %q, want it to name the packet that mounts the read path", problem.Detail)
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
	if strings.Contains(problem.Detail, "pantry-02") {
		t.Errorf("detail is %q, want it to name the read failure rather than the missing catalog", problem.Detail)
	}
	if !strings.Contains(problem.Detail, "deadline exceeded") {
		t.Errorf("detail is %q, want it to carry the underlying cause", problem.Detail)
	}
}

// The two data routes answer the document's declared 503 in this build, and
// never a 200 with an empty body. An empty `data: []` is indistinguishable from
// a platform with no services, which is the exact confusion the document names.
func TestTheDataRoutesAreFiveOhThreeAndNotAnEmptyRegistry(t *testing.T) {
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
// the whole list instead. This build answers 503 for the data routes, so the
// assertion is that it refuses rather than serves — the refusal is the part that
// has to survive pantry-02.
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

// panicOnCount is the fault injection for the panic path, and it replaces the
// CATALOG rather than the router: the request still passes through chi, still
// gets a trace id, still recovers — the whole chain production uses.
type panicOnCount struct{}

func (panicOnCount) List(context.Context, catalog.Filter) ([]api.Service, *string, error) {
	return nil, nil, nil
}
func (panicOnCount) Get(context.Context, string) (api.Service, error) { return api.Service{}, nil }
func (panicOnCount) Count(context.Context) (int, error)               { panic("injected") }
