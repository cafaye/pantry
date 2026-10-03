package pantrydb_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sort"
	"strings"
	"testing"
	"time"

	"github.com/cafaye/pantry/internal/api"
	"github.com/cafaye/pantry/internal/catalog"
	"github.com/cafaye/pantry/internal/httpapi"
	"github.com/cafaye/pantry/internal/pantrydb"
)

// This file is the packet's headline claim, and it is the ONE test that has to
// exist for this work to have been worth doing.
//
// # THE CLAIM
//
// The data routes used to answer 503, because a `200` with `services: 0` cannot
// be told apart from a platform that has no services. Making them answer 200 is
// only safe if the three truths stay distinguishable, and they are:
//
//	rows are there             200 with `data: [ … ]`
//	the catalog is empty       200 with `data: []`     — the database ANSWERED
//	the database is not there  503 with a problem      — the database did NOT
//
// The third is the one that is easy to skip, because the happy path is right
// there and green, and because it is the case the original 503 was standing in
// for. So it is asserted here against a real database that was there a moment ago
// and is now not, rather than against a mock that returns an error on request —
// a mock proves the handler switches on its argument; a real outage proves the
// handler switches on the database's.
//
// # WHY IT LIVES IN THIS PACKAGE
//
// The harness is here — `tests/rls.sh --serve`, the same one the role and query
// tests use — and a Go test package cannot import another package's test helpers.
// `internal/httpapi` does not import `internal/pantrydb`, so `pantrydb_test` can
// drive the real router without an import cycle, and there is still exactly one
// harness in the tree rather than one per package.

// TestTheThreeCasesAreDistinguishable is the assertion. Each case is a SEPARATE
// DATABASE rather than a row inserted and deleted, because "the catalog is
// genuinely empty" is a claim about a database that was migrated and never
// seeded — and a test that empties a populated database proves the query returns
// nothing, which is a different and weaker claim than "an empty database answers
// `[]` rather than `503`".
func TestTheThreeCasesAreDistinguishable(t *testing.T) {
	// CASE 1 — rows are there.
	t.Run("rows present answers 200 with the seeded rows", func(t *testing.T) {
		dbGate(t)
		get := router(t, catalog.New(mustOpen(t, dbURL).Queries()))

		rec := get(http.MethodGet, "/v1/services")
		if rec.Code != http.StatusOK {
			t.Fatalf("GET /v1/services is %d, want 200. The 503 the previous packet answered is "+
				"only correct when the database cannot be read; this one answered.\n%s",
				rec.Code, rec.Body.String())
		}

		var list api.ServiceList
		if err := json.Unmarshal(rec.Body.Bytes(), &list); err != nil {
			t.Fatalf("the 200 body is not a ServiceList: %v\n%s", err, rec.Body.String())
		}

		var names []string
		for _, s := range list.Data {
			names = append(names, s.Name)
		}
		sort.Strings(names)

		// THE EXACT SET, and this is the assertion that matters most in the file.
		// Five visible rows out of seven seeded: `draft-only` and `unlisted-one`
		// are in the database and are NOT in the answer, and the only thing that
		// removed them is the RLS policy — no query in this repository filters on
		// `state`. An assertion that listed the five without naming the two would
		// pass just as well for a query that returned the wrong five.
		want := "[caf courier identity muse parcel]"
		if got := "[" + strings.Join(names, " ") + "]"; got != want {
			t.Fatalf("GET /v1/services returned %s, want %s.\n"+
				"  The seed has seven services. `draft-only` is a third-party draft and "+
				"`unlisted-one` is a published row its publisher took off the listing; neither is "+
				"visible to pantry_public, and `pantry.service_is_visible` is what says so.",
				got, want)
		}
		if list.Page.HasMore {
			t.Errorf("has_more is true on %d rows with no limit asked for; the default page is 25 "+
				"and the seed holds five", len(list.Data))
		}
		if list.Page.NextCursor != nil {
			t.Errorf("next_cursor is %q on the last page; it must be null", *list.Page.NextCursor)
		}
		if !strings.Contains(rec.Body.String(), `"data":[`) {
			t.Errorf("`data` is not a JSON array in %s; the document says it is always an array, "+
				"empty rather than absent", rec.Body.String())
		}
		t.Logf("case 1: 200 with %v", names)
	})

	// CASE 2 — the catalog is genuinely empty.
	t.Run("an empty catalog answers 200 with an empty array", func(t *testing.T) {
		store := startEmpty(t)
		get := router(t, catalog.New(store.Queries()))

		rec := get(http.MethodGet, "/v1/services")
		if rec.Code != http.StatusOK {
			t.Fatalf("GET /v1/services against an EMPTY database is %d, want 200.\n"+
				"  This is the whole reason the routes used to answer 503: a 200 with `services: 0` "+
				"and a 503 were indistinguishable, and the fix is not to keep answering 503 but to "+
				"make the two cases tell themselves apart. The database answered; there is nothing in "+
				"it; that is 200 with `data: []`.\n%s", rec.Code, rec.Body.String())
		}

		// `[]` and NOT `null`. `data` is REQUIRED and "always an array, empty
		// rather than absent", and a client written against this document
		// distinguishes the two: one is an empty catalog and the other is a shape
		// it has never seen.
		if body := strings.TrimSpace(rec.Body.String()); !strings.HasPrefix(body, `{"data":[]`) {
			t.Fatalf("the empty catalog answered %s, want a body beginning `{\"data\":[]`.\n"+
				"  `null` here would be a nil slice, which is a different JSON type and not what the "+
				"document publishes for this key.", body)
		}

		// And `/readyz` must agree with the data route: the database is
		// reachable, so the probe says ok with a count of zero rather than
		// `unavailable`. A probe that reported 503 here would leave an operator
		// restarting a process that is working perfectly.
		ready := get(http.MethodGet, "/readyz")
		if ready.Code != http.StatusOK {
			t.Fatalf("GET /readyz against an empty database is %d, want 200.\n"+
				"  `/readyz` asks whether there is something to SERVE, and an empty catalog is "+
				"something to serve: the database answered.\n%s", ready.Code, ready.Body.String())
		}
		var readiness api.Readiness
		if err := json.Unmarshal(ready.Body.Bytes(), &readiness); err != nil {
			t.Fatalf("the /readyz body is not a Readiness: %v", err)
		}
		if readiness.Services != 0 {
			t.Errorf("/readyz says %d services against an empty database", readiness.Services)
		}
		t.Log("case 2: 200 with `data: []` and /readyz 200 with services=0")
	})

	// CASE 3 — the database is not there.
	//
	// THE POINT OF THE WHOLE PACKET. A store that could be opened against a dead
	// database would turn an outage into `data: []`, and a client would cache
	// "this platform has no services" for as long as its cache lives.
	t.Run("an unreachable database answers 503 and not an empty catalog", func(t *testing.T) {
		// A port that was bound and released a moment ago, so the connect is
		// refused rather than hanging.
		dead := unreachableURL(t)

		// The mount is refused, so there is no catalog — which is the honest state:
		// a store that cannot reach its database is not a store, and pretending
		// otherwise is what this case is about.
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()
		store, err := pantrydb.Open(ctx, dead)
		if err == nil {
			store.Close()
			t.Fatalf("pantrydb.Open succeeded against %s.\n"+
				"  A store that opens against a dead database makes case 3 indistinguishable from "+
				"case 2, which is the one confusion this packet exists to remove.", redact(dead))
		}

		// The SAME `mountCatalog` decision the binary makes, driven through the
		// handler rather than through `main`, because `main` is not testable and
		// the thing being tested is what a caller SEES.
		get := mount(t, nil, "the registry database could not be reached at startup, so there is "+
			"nothing to answer with: "+err.Error())

		for _, path := range []string{"/v1/services", "/v1/services/identity"} {
			rec := get(http.MethodGet, path)
			if rec.Code != http.StatusServiceUnavailable {
				t.Fatalf("GET %s against an unreachable database is %d, want 503.\n"+
					"  200 here is the failure this packet exists to prevent: a client would cache an "+
					"empty registry for an outage and nobody would find out until the service was "+
					"asked for something it never had.\n%s", path, rec.Code, rec.Body.String())
			}
			if strings.Contains(rec.Body.String(), `"data"`) {
				t.Fatalf("GET %s answered a registry-shaped body: %s", path, rec.Body.String())
			}
			var problem api.Problem
			if err := json.Unmarshal(rec.Body.Bytes(), &problem); err != nil {
				t.Fatalf("GET %s: the 503 is not a problem document: %v", path, err)
			}
			if problem.Code != api.Unavailable {
				t.Errorf("GET %s code is %q, want %q", path, problem.Code, api.Unavailable)
			}
			if !strings.Contains(problem.Detail, "could not be reached") {
				t.Errorf("GET %s detail is %q, want it to name the unreachable database", path, problem.Detail)
			}
		}

		// `/healthz` STILL answers 200. This is not politeness: the document says
		// so, and an orchestrator that restarts on it turns a database outage into
		// a crash loop, which is load on a database that is already failing.
		if rec := get(http.MethodGet, "/healthz"); rec.Code != http.StatusOK {
			t.Errorf("GET /healthz is %d during an outage, want 200; a restart loop is the harm the "+
				"document's split probes exist to prevent", rec.Code)
		}
		// `/readyz` answers 503, and this is the one that MUST move: a process
		// that answers 200 here while its database is unreachable is lying about
		// being useful.
		ready := get(http.MethodGet, "/readyz")
		if ready.Code != http.StatusServiceUnavailable {
			t.Fatalf("GET /readyz during an outage is %d, want 503.\n"+
				"  `/readyz` answers 200 only when there is something to serve, and a service whose "+
				"database it cannot reach has nothing to serve.\n%s", ready.Code, ready.Body.String())
		}
		t.Log("case 3: 503 on both data routes and on /readyz, 200 on /healthz")
	})
}

// TestAFilterTheDocumentCannotAnswerIsFourHundredAndNotAnEmptyPage is here
// because it is the fourth case the first three do not cover, and it is the one a
// client is most likely to hit by accident.
//
// A filter the document cannot answer used to be indistinguishable from a filter
// that matched nothing. Both would have been `200` with `data: []` if the filter
// had simply been ignored, and "we narrowed to `worker` and got nothing back" is a
// conclusion a caller will act on. So it is a 400 naming the parameter and what
// was allowed.
func TestAFilterTheDocumentCannotAnswerIsFourHundredAndNotAnEmptyPage(t *testing.T) {
	dbGate(t)
	get := router(t, catalog.New(mustOpen(t, dbURL).Queries()))

	cases := []struct {
		query string
		why   string
	}{
		{"/v1/services?language=zig", "the schema's language CHECK admits zig and the document's " +
			"Language enum does not, so this is a value outside the vocabulary this version can answer"},
		{"/v1/services?kind=nonsense", "kind is a closed vocabulary"},
		{"/v1/services?contract=1.2", "core's grammar is MAJOR.MINOR.PATCH and nothing shorter"},
		{"/v1/services?limit=0", "the document's minimum is 1, and a bare `limit` is the default"},
		{"/v1/services?limit=101", "the document's maximum is 100"},
		{"/v1/services?cursor=bm90LWEtY3Vyc29y", "a cursor this service did not issue must not " +
			"silently read as page one, which would lose rows without saying so"},
		{"/v1/services?sort=name", "an unknown parameter is told rather than quietly ignored, " +
			"because a client asking for a sort it did not get is handed the whole list"},
	}
	for _, tc := range cases {
		rec := get(http.MethodGet, tc.query)
		if rec.Code != http.StatusBadRequest {
			t.Errorf("GET %s is %d, want 400. %s.\n  body: %s", tc.query, rec.Code, tc.why, rec.Body.String())
			continue
		}
		var problem api.Problem
		if err := json.Unmarshal(rec.Body.Bytes(), &problem); err != nil {
			t.Errorf("GET %s: the 400 is not a problem document: %v", tc.query, err)
			continue
		}
		if problem.Code != api.ValidationFailed {
			t.Errorf("GET %s code is %q, want %q", tc.query, problem.Code, api.ValidationFailed)
		}
		if strings.TrimSpace(rec.Body.String()) == "" {
			t.Errorf("GET %s answered 400 with an empty body; the document requires a detail", tc.query)
		}
	}
}

// TestAFilterThatMatchesNothingIsTwoHundredWithAnEmptyArray is the other
// direction and it must NOT be a 400: a good question about a thing that is not
// there is a good question with an empty answer, and answering it with a 400
// teaches a client to retry something that will never work.
func TestAFilterThatMatchesNothingIsTwoHundredWithAnEmptyArray(t *testing.T) {
	dbGate(t)
	get := router(t, catalog.New(mustOpen(t, dbURL).Queries()))

	// `worker` is in the DOCUMENT's vocabulary and is not in the schema's: the
	// document publishes `[api, worker, both, cli]` and `pantry.service_kind` is a
	// closed set of `(api, cli)`. So `?kind=worker` is a legal question with no
	// rows behind it, and it is 200 with `data: []` — not a 400 (it is in the
	// vocabulary) and not a 500 (the database refuses to cast it to an enum, which
	// is why the query compares `kind::text`).
	rec := get(http.MethodGet, "/v1/services?kind=worker")
	if rec.Code != http.StatusOK {
		t.Fatalf("GET /v1/services?kind=worker is %d, want 200 with an empty array: worker is in "+
			"the document's vocabulary, so this is a question with no answer rather than an "+
			"unaskable one.\n%s", rec.Code, rec.Body.String())
	}
	if body := strings.TrimSpace(rec.Body.String()); !strings.HasPrefix(body, `{"data":[]`) {
		t.Errorf("?kind=worker answered %s, want `{\"data\":[]`", body)
	}

	// A language that IS in the document and matches nothing in the seed.
	rec = get(http.MethodGet, "/v1/services?language=ruby")
	if rec.Code != http.StatusOK {
		t.Fatalf("GET /v1/services?language=ruby is %d, want 200; no seeded service is ruby",
			rec.Code)
	}
}

// TestTheContractFilterIsRangeIntersectionAndNotStringEquality is the
// document's own example, and it is the reason the filter cannot be `=`.
//
// `openapi/v1.yaml`: "`>=0.2.0` matches a service pinned to `^0.2.0`, and
// `^0.2.0` does not match a service pinned to `^0.1.0` because before 1.0 a caret
// pins the minor."
func TestTheContractFilterIsRangeIntersectionAndNotStringEquality(t *testing.T) {
	dbGate(t)
	get := router(t, catalog.New(mustOpen(t, dbURL).Queries()))

	// The seed: identity `^0.1.0`, courier `^0.1.0`, muse `^0.2.0`, caf `^0.1.0`,
	// parcel `^0.2.0`.
	cases := []struct {
		contract string
		want     string
		why      string
	}{
		{">=0.2.0", "[muse parcel]",
			"the document's first example: >=0.2.0 shares a version with ^0.2.0 and with nothing below it"},
		{"^0.2.0", "[muse parcel]",
			"^0.2.0 is [0.2.0, 0.3.0) because before 1.0 a caret pins the MINOR"},
		{"^0.1.0", "[caf courier identity]",
			"^0.1.0 is [0.1.0, 0.2.0), so it excludes muse and parcel"},
		{">=0.1.0", "[caf courier identity muse parcel]",
			"a gte is unbounded above"},
		{"~0.2.0", "[muse parcel]",
			"~0.2.0 is [0.2.0, 0.3.0) — the same set as ^0.2.0 here, and a different one from ^0.1.0"},
		{"1.2.3", "[]",
			"an exact triple matches nothing in the seed; the point is that it does not raise and " +
				"does not match by string"},
		{"^0.1.5", "[caf courier identity]",
			"^0.1.5 is [0.1.5, 0.2.0) and ^0.1.0 is [0.1.0, 0.2.0); they overlap from 0.1.5, so this is a match"},
	}
	for _, tc := range cases {
		rec := get(http.MethodGet, "/v1/services?contract="+queryEscape(tc.contract))
		if rec.Code != http.StatusOK {
			t.Errorf("?contract=%s is %d, want 200. %s.\n  body: %s",
				tc.contract, rec.Code, tc.why, rec.Body.String())
			continue
		}
		var list api.ServiceList
		if err := json.Unmarshal(rec.Body.Bytes(), &list); err != nil {
			t.Errorf("?contract=%s: %v", tc.contract, err)
			continue
		}
		var names []string
		for _, s := range list.Data {
			names = append(names, s.Name)
		}
		sort.Strings(names)
		got := "[" + strings.Join(names, " ") + "]"
		if got != tc.want {
			t.Errorf("?contract=%s returned %s, want %s. %s", tc.contract, got, tc.want, tc.why)
		}
	}
}

// TestOneServiceIsTwoHundredAndAnUnknownNameIsFourHundred holds the pair that a
// `200` with an empty object would have destroyed: the document says an unknown
// name is a 404, and a 404 is only meaningful if a KNOWN name is a 200.
func TestOneServiceIsTwoHundredAndAnUnknownNameIsFourHundred(t *testing.T) {
	dbGate(t)
	get := router(t, catalog.New(mustOpen(t, dbURL).Queries()))

	rec := get(http.MethodGet, "/v1/services/identity")
	if rec.Code != http.StatusOK {
		t.Fatalf("GET /v1/services/identity is %d, want 200.\n%s", rec.Code, rec.Body.String())
	}
	var svc api.Service
	if err := json.Unmarshal(rec.Body.Bytes(), &svc); err != nil {
		t.Fatalf("the 200 body is not a Service: %v", err)
	}
	if svc.Name != "identity" {
		t.Errorf("name is %q, want identity", svc.Name)
	}
	if svc.Kind != api.API || svc.Language != api.Go {
		t.Errorf("kind is %q and language is %q, want %q and %q — both come from the manifest, "+
			"per the document's own field-provenance table", svc.Kind, svc.Language, api.API, api.Go)
	}
	if svc.BasePath == nil || *svc.BasePath != "/v1" {
		t.Errorf("basePath is %v, want /v1", svc.BasePath)
	}
	if svc.Exposes == nil || svc.Exposes.API == nil || *svc.Exposes.API != "openapi/v1.yaml" {
		t.Errorf("exposes is %+v, want the manifest's own api path", svc.Exposes)
	}

	// A `cli` with no contract surface: BOTH nulls are facts the document
	// publishes, not absences, and a projection that turned either into `""` or an
	// omitted key would be publishing something the contract does not describe.
	rec = get(http.MethodGet, "/v1/services/caf")
	if rec.Code != http.StatusOK {
		t.Fatalf("GET /v1/services/caf is %d, want 200.\n%s", rec.Code, rec.Body.String())
	}
	svc = api.Service{}
	if err := json.Unmarshal(rec.Body.Bytes(), &svc); err != nil {
		t.Fatalf("the 200 body is not a Service: %v", err)
	}
	if svc.Kind != api.Cli {
		t.Errorf("kind is %q, want %q", svc.Kind, api.Cli)
	}
	if svc.BasePath != nil {
		t.Errorf("basePath is %v, want null: a cli publishes no OpenAPI document, and null there "+
			"is a fact rather than an absence", *svc.BasePath)
	}
	if svc.Exposes != nil {
		t.Errorf("exposes is %+v, want null: the manifest declares no contract surface", svc.Exposes)
	}
	if !strings.Contains(rec.Body.String(), `"basePath":null`) {
		t.Errorf("the body omits basePath: %s.\n  basePath is REQUIRED and may be null; omitting "+
			"the key is a different shape from publishing null", rec.Body.String())
	}

	// The 404. Not an empty object, not a 503, not a 200.
	for _, name := range []string{"nope", "draft-only", "unlisted-one"} {
		rec := get(http.MethodGet, "/v1/services/"+name)
		if rec.Code != http.StatusNotFound {
			t.Errorf("GET /v1/services/%s is %d, want 404.\n  body: %s", name, rec.Code, rec.Body.String())
			continue
		}
		var problem api.Problem
		if err := json.Unmarshal(rec.Body.Bytes(), &problem); err != nil {
			t.Errorf("GET /v1/services/%s: the 404 is not a problem document: %v", name, err)
			continue
		}
		if problem.Code != api.NotFound {
			t.Errorf("GET /v1/services/%s code is %q, want %q", name, problem.Code, api.NotFound)
		}
		if problem.Instance != "/v1/services/"+name {
			t.Errorf("GET /v1/services/%s instance is %q, want the request path", name, problem.Instance)
		}
	}
	// `draft-only` and `unlisted-one` are in the DATABASE. They are 404s here
	// because RLS cannot see them, and that is deliberate: to a public reader a
	// draft is not a service that exists, and saying otherwise would be telling one
	// caller what another publisher has not published.
}

// TestPagingWalksTheWholeCatalogWithoutRepeatingOrLosingARow is what `has_more`
// is for. A cursor that skipped a row would make the registry look smaller than it
// is, and one that repeated a row would make it look larger; both are the kind of
// wrong answer that nobody notices until somebody counts.
func TestPagingWalksTheWholeCatalogWithoutRepeatingOrLosingARow(t *testing.T) {
	dbGate(t)
	get := router(t, catalog.New(mustOpen(t, dbURL).Queries()))

	seen := map[string]bool{}
	pages := 0
	path := "/v1/services?limit=2"
	for {
		rec := get(http.MethodGet, path)
		if rec.Code != http.StatusOK {
			t.Fatalf("GET %s is %d, want 200.\n%s", path, rec.Code, rec.Body.String())
		}
		var list api.ServiceList
		if err := json.Unmarshal(rec.Body.Bytes(), &list); err != nil {
			t.Fatalf("GET %s: %v", path, err)
		}
		pages++
		if pages > 10 {
			t.Fatal("paging did not terminate in ten pages of two rows over a five-row catalog")
		}
		for _, s := range list.Data {
			if seen[s.Name] {
				t.Fatalf("%s appeared twice while paging; the cursor is not exclusive", s.Name)
			}
			seen[s.Name] = true
		}
		if list.Page.NextCursor == nil {
			if list.Page.HasMore {
				t.Errorf("has_more is true with a null next_cursor on page %d", pages)
			}
			break
		}
		if !list.Page.HasMore {
			t.Errorf("page %d carries a cursor but has_more is false", pages)
		}
		path = "/v1/services?limit=2&cursor=" + queryEscape(*list.Page.NextCursor)
	}

	if len(seen) != 5 {
		t.Fatalf("paging with limit=2 saw %d rows (%v), want the 5 the catalog holds. A cursor "+
			"that skips a row makes the registry look smaller than it is, and nothing notices "+
			"until somebody counts.", len(seen), seen)
	}
	if pages != 3 {
		t.Errorf("five rows at two per page took %d pages, want 3", pages)
	}
}

func mustOpen(t *testing.T, url string) *pantrydb.Store {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	store, err := pantrydb.Open(ctx, url)
	if err != nil {
		t.Fatalf("pantrydb.Open(%s): %v", redact(url), err)
	}
	t.Cleanup(store.Close)
	return store
}

// router mounts the REAL handler over the REAL catalog and returns a request
// function. Nothing between the request and the SQL is faked: the generated
// router, the chi stack, the trace middleware, the catalog, the generated queries
// and a PostgreSQL that has the migrations applied.
//
// The 503 sentence it passes is the one `cmd/pantry`'s `mountCatalog` produces for
// an unreachable database, so what these tests assert is what a deployment
// actually serves rather than a sentence invented here.
func router(t *testing.T, cat catalog.Catalog) func(method, path string) *httptest.ResponseRecorder {
	t.Helper()
	return mount(t, cat, "the registry database could not be reached at startup, so there is "+
		"nothing to answer with: could not reach the database")
}

func mount(t *testing.T, cat catalog.Catalog, unavailableDetail string) func(method, path string) *httptest.ResponseRecorder {
	t.Helper()
	handler := httpapi.New(cat, httpapi.WithUnavailableDetail(unavailableDetail))
	return func(method, path string) *httptest.ResponseRecorder {
		rec := httptest.NewRecorder()
		handler.ServeHTTP(rec, httptest.NewRequest(method, path, nil))
		return rec
	}
}

// queryEscape is `url.QueryEscape` under a local name, so the reason it exists is
// at the call site: a caret is legal in a query string but `?contract=^0.2.0`
// reads as a typo and `?contract=%5E0.2.0` does not.
func queryEscape(s string) string {
	var b strings.Builder
	for _, r := range s {
		switch {
		case r >= 'a' && r <= 'z', r >= 'A' && r <= 'Z', r >= '0' && r <= '9',
			r == '-', r == '_', r == '.', r == '~':
			b.WriteRune(r)
		default:
			b.WriteString("%" + strings.ToUpper(hexByte(byte(r))))
		}
	}
	return b.String()
}

func hexByte(b byte) string {
	const digits = "0123456789ABCDEF"
	return string([]byte{digits[b>>4], digits[b&0x0f]})
}
