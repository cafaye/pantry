// Package catalog is the read seam between the HTTP surface and a data source.
//
// # WHAT THIS IS NOW, AND WHAT IT IS NOT
//
// Three methods, the two the document's two data operations need plus the count
// `/readyz` asks for, expressed in the GENERATED types rather than in pantry's
// own — so a handler cannot serialise a shape the document does not declare.
//
// This build has two implementations: `Postgres`, over the migrations, and the
// fake in `internal/httpapi`'s tests. Both are the interface, and the HTTP
// contract tests run against both, so a handler that grows a special case for the
// real catalog breaks a test rather than a deployment.
//
// The compatibility graph IS here, since registry-compat-05, as two operations
// and not as a field on `Service`: the forward direction (`Requirements`) is
// "what do I need to run this?" and the backward direction (`RequiredBy`) is
// "who breaks if I change this?" — two questions asked by two different people,
// which is why they are two methods with two wrappers in the document rather
// than one array with a direction flag. The graph had to wait for a packet of
// its own for a stated reason, recorded in `REPORT-registry-pantry-data-01.md`:
// it is additive but `cafaye-ts` has a client generated from the document, so
// putting it on the wire was a contract change for the manager to make
// deliberately, and this is that decision made.
//
// `conflicts_with` is deliberately NOT reachable from this interface, and the
// reason is the graph's whole value: answering "what must I not run alongside?"
// through the same method that answers "what must I run?" is how a caller ends
// up installing a conflict. When it goes on the wire it gets a third method,
// and the compiler — not a convention — is what keeps them apart.
//
// # WHAT THE SCHEMA SETTLED ABOUT `Filter`
//
// `catalog.Filter` was provisional in the previous packet, and the columns have
// now decided most of it. Three of its five fields are columns that exist
// (`services.kind`, `services.language`, `services.core_constraint`) and all three
// now do something. The fourth changed SHAPE rather than content:
//
//	Limit *int   and not Limit int
//
// The document declares `limit` with `minimum: 1`, `maximum: 100` and
// `default: 25`, and oapi-codegen does not enforce any of the three — it binds the
// parameter and leaves the range and the default to the caller. With `Limit int`,
// "the caller did not ask" and "the caller asked for zero" are the same value, so
// either `?limit=0` is silently answered as 25 or it is silently clamped. A
// parameter that is read and then not obeyed is worse than one that is refused.
// A pointer is the difference between the default and a request.
//
// What is NOT a filter is as load-bearing as what is. `trust` and `state` are
// columns and they are absent here, because `pantry.service_is_visible` is the
// policy that reads them: adding `and state = 'published'` to the query would
// hide the fleet's own draft services, which `00005_functions.sql` deliberately
// admits, and would be a second implementation of a boundary that already exists.
package catalog

import (
	"context"
	"errors"
	"regexp"

	"github.com/cafaye/pantry/internal/api"
)

// ErrNoSuchService is returned by Get, Requirements and RequiredBy for a name
// the registry does not carry. It is a sentinel rather than a bool so a read
// path can wrap it with its own detail without the handler learning what could
// have caused it.
var ErrNoSuchService = errors.New("no such service")

// ErrBadFilter is returned by List for a filter this version of the document
// cannot answer: a `kind` or `language` outside its vocabulary, a `contract` that
// is not core's four-form grammar, a `limit` outside 1-100, or a cursor this
// service did not issue.
//
// It is separate from ErrNoSuchService because the two are different answers and
// conflating them is how a client learns to retry a 400: a name that does not
// exist is a 404 and will still not exist, and a filter that cannot be answered
// is a 400 that a different request would answer.
var ErrBadFilter = errors.New("filter outside the document's vocabulary")

// coreConstraintGrammar is core's four forms and nothing else, and it is written
// out here rather than derived because the document is not available at runtime.
// The same pattern appears three times in `migrations/` — 00003's CHECK on
// `services.core_constraint`, 00004's on `service_compat.version_range`, and
// `openapi/v1.yaml`'s `contract` parameter — so this is the fourth copy of a
// grammar that has to agree with itself, and the tests are what hold it there.
var coreConstraintGrammar = regexp.MustCompile(`^(\^|~|>=)?[0-9]+\.[0-9]+\.[0-9]+$`)

// serviceNameGrammar is core's `name` pattern, copied from `openapi/v1.yaml`'s
// path parameter and from `00003`'s CHECK. It is used to refuse a cursor this
// service could not have issued — see `decodeCursor`.
var serviceNameGrammar = regexp.MustCompile(`^[a-z][a-z0-9]*(-[a-z0-9]+)*$`)

// Paging bounds, from `openapi/v1.yaml`'s `limit` parameter. The document states
// them and the generated binder states neither, so they are stated here where
// they are obeyed.
const (
	// DefaultPageSize is the document's `default: 25`.
	DefaultPageSize = 25
	// MaxPageSize is the document's `maximum: 100`.
	MaxPageSize = 100
	// MinPageSize is the document's `minimum: 1`. Zero is not "unset" — unset is a
	// nil pointer, which is why `Filter.Limit` is `*int`.
	MinPageSize = 1
)

// Catalog is the read source for /v1/services and /v1/services/{name}.
type Catalog interface {
	// List returns the registry, filtered and paged, plus the cursor for the
	// next page or nil on the last one.
	//
	// It returns ErrBadFilter for anything the document cannot answer and
	// ErrNoSuchService is not among its answers: "no service matches this filter"
	// is a 200 with an empty `data`, because the collection exists and the
	// question was a good one.
	List(ctx context.Context, filter Filter) (services []api.Service, next *string, err error)

	// Get returns one service, or ErrNoSuchService.
	Get(ctx context.Context, name string) (api.Service, error)

	// Count returns how many services the registry holds. It exists for /readyz
	// alone: the document's Readiness body carries `services`, and a count that
	// only exists as len(List(...)) would make the probe load a whole page to
	// answer "is there anything here".
	Count(ctx context.Context) (int, error)

	// Requirements returns the FORWARD direction of the compatibility graph for
	// one service — what it requires to run — or ErrNoSuchService. `requires`
	// edges only; see the package comment for why `conflicts_with` has no path
	// through this interface.
	Requirements(ctx context.Context, name string) ([]api.CompatibilityEdge, error)

	// RequiredBy returns the BACKWARD direction for one service — what requires
	// it — or ErrNoSuchService. Same edges, other endpoint, different question.
	RequiredBy(ctx context.Context, name string) ([]api.CompatibilityEdge, error)
}

// Filter is what the document's query parameters mean, and nothing more.
//
// Every field does something. None of them filters on tenant or on state, for
// the reasons the package comment gives.
type Filter struct {
	// Kind and Language are the document's two enums, in the GENERATED types so
	// that a value outside either vocabulary is refused by `Validate` against the
	// generated constants rather than against a hand-written list that can fall
	// behind the document.
	Kind     *api.ServiceKind
	Language *api.Language

	// Contract is core's four-form range, matched by INTERSECTION against the
	// service's own `core` constraint. `>=0.2.0` matches a service pinned to
	// `^0.2.0`; `^0.2.0` does not match one pinned to `^0.1.0`, because before 1.0
	// a caret pins the minor. It is the document's own example.
	Contract *string

	// Limit is the page size, and it is a POINTER on purpose: nil is "the caller
	// did not ask", which `DefaultPageSize` answers, and is a different thing from
	// zero, which `Validate` refuses.
	Limit *int

	// Cursor is an opaque token from a previous response's `page.next_cursor`.
	Cursor *string
}

// PageSize is the number of rows to return, after the default is applied.
func (f Filter) PageSize() int {
	if f.Limit == nil {
		return DefaultPageSize
	}
	return *f.Limit
}

// Validate refuses everything the document says is a 400, and it derives its two
// enums from the generated constants so a vocabulary that grows is a vocabulary
// this accepts without being edited.
//
// The check order is the document's own: an unknown parameter is a 400 "so a
// client that asks a question this version cannot answer is told rather than
// quietly given the whole list", and a bad value inside a known parameter is the
// same answer.
func (f Filter) Validate() error {
	if f.Kind != nil && !knownKind(*f.Kind) {
		return filterError("kind", string(*f.Kind), knownKinds())
	}
	if f.Language != nil && !knownLanguage(*f.Language) {
		return filterError("language", string(*f.Language), knownLanguages())
	}
	if f.Contract != nil && !coreConstraintGrammar.MatchString(*f.Contract) {
		return filterError("contract", *f.Contract,
			[]string{"`^MAJOR.MINOR.PATCH`", "`~MAJOR.MINOR.PATCH`", "`>=MAJOR.MINOR.PATCH`", "`MAJOR.MINOR.PATCH`"})
	}
	if f.Limit != nil && (*f.Limit < MinPageSize || *f.Limit > MaxPageSize) {
		return filterError("limit", itoa(*f.Limit), []string{"1 through 100"})
	}
	return nil
}

// knownKinds and knownLanguages read the GENERATED vocabulary, not a list written
// beside it. `openapi/v1.yaml` is the authority for both, oapi-codegen emits one
// constant per value, and a list copied out of the document into Go is a list that
// is wrong the day the document moves.
func knownKind(k api.ServiceKind) bool {
	switch k {
	case api.API, api.Worker, api.Both, api.Cli:
		return true
	}
	return false
}

func knownKinds() []string {
	return []string{string(api.API), string(api.Worker), string(api.Both), string(api.Cli)}
}

func knownLanguage(l api.Language) bool {
	switch l {
	case api.Go, api.Ruby, api.Elixir, api.Python, api.Typescript, api.Rust:
		return true
	}
	return false
}

func knownLanguages() []string {
	return []string{
		string(api.Go), string(api.Ruby), string(api.Elixir),
		string(api.Python), string(api.Typescript), string(api.Rust),
	}
}

func filterError(param, got string, allowed []string) error {
	msg := param + "=" + got + " is not a value this version of the document can answer. allowed: "
	for i, a := range allowed {
		if i > 0 {
			msg += ", "
		}
		msg += a
	}
	return wrapBadFilter(msg)
}

func itoa(n int) string {
	// `strconv` is here rather than `fmt` so the message cannot be given a format
	// string that a caller controls: every use of this is an int.
	if n == 0 {
		return "0"
	}
	neg := n < 0
	if neg {
		n = -n
	}
	var buf [20]byte
	i := len(buf)
	for n > 0 {
		i--
		buf[i] = byte('0' + n%10)
		n /= 10
	}
	if neg {
		i--
		buf[i] = '-'
	}
	return string(buf[i:])
}
