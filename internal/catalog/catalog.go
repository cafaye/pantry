// Package catalog is the read seam between the HTTP surface and a data source.
//
// # WHAT THIS IS, AND WHAT IT DELIBERATELY IS NOT
//
// Two methods, the two the document's two data operations need, expressed in
// the GENERATED types rather than in pantry's own — so a handler cannot
// serialise a shape the document does not declare.
//
// What is NOT here is everything that would make this a database design: the
// filter type, the cursor encoding, the compatibility rules behind `contract`,
// the ingest that turns git manifests into rows. Those are pantry-02, and a
// schema decision made from this packet's seat would be wrong in the one place
// the addendum says the moat lives. This interface is deliberately the smallest
// thing that lets pantry-02 mount a real source and lets a test drive a fake;
// if pantry-02 needs a different shape, changing it here is a two-line commit
// and the tests that hold the HTTP contract do not move.
package catalog

import (
	"context"
	"errors"

	"github.com/cafaye/pantry/internal/api"
)

// ErrNoSuchService is returned by Get for a name the registry does not carry.
// It is a sentinel rather than a bool so a read path can wrap it with its own
// detail without the handler learning what could have caused it.
var ErrNoSuchService = errors.New("no such service")

// Catalog is the read source for /v1/services and /v1/services/{name}.
type Catalog interface {
	// List returns the registry, filtered and paged, plus the cursor for the
	// next page or nil on the last one.
	//
	// Provisional: the filter and cursor parameters are named here because the
	// document declares them, and their meaning is pantry-02's to settle.
	List(ctx context.Context, filter Filter) (services []api.Service, next *string, err error)

	// Get returns one service, or ErrNoSuchService.
	Get(ctx context.Context, name string) (api.Service, error)

	// Count returns how many services the registry holds. It exists for /readyz
	// alone: the document's Readiness body carries `services`, and a count that
	// only exists as len(List(...)) would make the probe load the whole registry
	// to answer "is there anything here".
	Count(ctx context.Context) (int, error)
}

// Filter is what the document's query parameters mean, and nothing more.
type Filter struct {
	Kind     *api.ServiceKind
	Language *api.Language
	Contract *string
	Limit    int
	Cursor   *string
}
