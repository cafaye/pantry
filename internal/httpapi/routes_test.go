package httpapi

import (
	"net/http"
	"os"
	"regexp"
	"sort"
	"strings"
	"testing"

	"github.com/go-chi/chi/v5"

	"github.com/cafaye/pantry/internal/api"
)

// THE TWO DIRECTIONS A COMPILER CANNOT HOLD.
//
// A path in openapi/v1.yaml with no implementation is a build error, because
// the generated api.ServerInterface is what Service satisfies: adding an
// operationId adds a method the implementation does not have. The reverse — a
// route served that the document does not declare — is not an error anywhere, so
// it is asserted here: the document is the contract, and a route outside it is
// an endpoint this service publishes and does not own.
//
// The Rust service asserted the same pair in `tests/api.rs`, which was deleted
// with `src/`. There is one implementation of this document now, so this file is
// the only place either direction is asserted — which is exactly why it cannot be
// deleted along with a refactor that made the check look obvious.

// openAPIDocument is the document, read at the path relative to this package.
// Embedding it with go:embed would pin a COPY of the file into this binary, and
// a copy is what this repository's own AGENTS.md calls a copy nobody checks.
const openAPIDocument = "../../openapi/v1.yaml"

// documentPath is one path under `paths:`, indented by exactly two spaces in the
// document. A crude scan rather than a YAML parse, deliberately: pulling in a
// YAML library to read a document the GENERATOR already parsed would add a
// dependency to the suite for no new assertion.
var documentPath = regexp.MustCompile(`(?m)^  (/[^:]*):$`)

func documentPaths(t *testing.T) map[string]bool {
	t.Helper()
	raw, err := os.ReadFile(openAPIDocument)
	if err != nil {
		t.Fatalf("reading %s: %v — the Go service is generated from this document, so a "+
			"service that cannot read it is a service whose contract is missing", openAPIDocument, err)
	}
	paths := map[string]bool{}
	for _, m := range documentPath.FindAllStringSubmatch(string(raw), -1) {
		paths[m[1]] = true
	}
	if len(paths) == 0 {
		t.Fatalf("%s declares no paths; the pattern in this file no longer matches the document", openAPIDocument)
	}
	return paths
}

// mountedRoutes walks the router the service actually serves. It returns
// "METHOD /pattern" for every registered route, which is what chi's patterns
// look like — the {name} in the document is the {name} here, and the comparison
// is textual because both sides come from the same document.
func mountedRoutes(t *testing.T) []string {
	t.Helper()
	h := New(nil)
	router, ok := h.(chi.Router)
	if !ok {
		t.Fatalf("the handler is a %T, not a chi.Router; this test cannot walk it", h)
	}
	var routes []string
	chi.Walk(router, func(method, route string, _ http.Handler, _ ...func(http.Handler) http.Handler) error {
		routes = append(routes, method+" "+route)
		return nil
	})
	sort.Strings(routes)
	return routes
}

func TestEveryRouteServedIsDeclaredInTheDocument(t *testing.T) {
	declared := documentPaths(t)
	for _, route := range mountedRoutes(t) {
		_, pattern, found := strings.Cut(route, " ")
		if !found {
			t.Fatalf("malformed route %q", route)
		}
		if !declared[pattern] {
			t.Errorf("the router serves %s, which openapi/v1.yaml does not declare. A route "+
				"outside the document is an endpoint this service publishes and does not own.", route)
		}
	}
}

func TestEveryPathInTheDocumentIsMounted(t *testing.T) {
	mounted := map[string]bool{}
	for _, route := range mountedRoutes(t) {
		_, pattern, _ := strings.Cut(route, " ")
		mounted[pattern] = true
	}
	for path := range documentPaths(t) {
		if !mounted[path] {
			t.Errorf("%s is declared in openapi/v1.yaml and not mounted. The document is the "+
				"routing table; an unmounted path is a contract this service does not keep.", path)
		}
	}
}

// The document declares four operations and every one is a GET. This is
// pantry's rule as much as the document's — the deleted `tests/scoping.rs`
// asserted the same about the Rust service — and a write verb appearing here is a
// new capability
// in a rewrite, which is a reviewable decision and not a refactor.
func TestEveryMountedRouteIsARead(t *testing.T) {
	for _, route := range mountedRoutes(t) {
		method, _, _ := strings.Cut(route, " ")
		if method != http.MethodGet {
			t.Errorf("the router serves %s, and pantry mounts reads only", route)
		}
	}
}

// The four operations the document declares, named. A list rather than a count,
// because a count cannot say WHICH route appeared, and this repository has been
// caught by a gate that counted what it should have named (bin/prime's test
// binary count, gate.yml's floor on it).
func TestTheRouterServesTheFourDeclaredOperations(t *testing.T) {
	got := mountedRoutes(t)
	want := []string{
		"GET /healthz",
		"GET /readyz",
		"GET /v1/services",
		"GET /v1/services/{name}",
	}
	if len(got) != len(want) {
		t.Fatalf("the router serves %d routes %v, want %d %v", len(got), got, len(want), want)
	}
	for i, route := range want {
		if got[i] != route {
			t.Errorf("route %d is %q, want %q (all: %v)", i, got[i], route, got)
		}
	}
}

// The generated wrapper is what turns a bad query parameter into an error
// rather than a panic, so its presence is asserted rather than assumed: a
// document that grows a typed parameter and a wrapper that stopped decoding it
// would otherwise show up as a 500 in production and nowhere in the suite.
func TestTheGeneratedWrapperIsTheOneBoundToTheService(t *testing.T) {
	var _ api.ServerInterface = (*Service)(nil)
	var _ api.ServerInterface = api.Unimplemented{}
}
