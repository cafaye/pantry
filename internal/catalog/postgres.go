package catalog

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"strings"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgtype"

	"github.com/cafaye/pantry/internal/api"
	gen "github.com/cafaye/pantry/internal/pantrydb/gen"
)

// Postgres is `Catalog` over the migrations.
//
// It holds generated queries and nothing else: every statement it runs is in
// `internal/pantrydb/queries/catalog.sql`, and the row types it reads are the
// generated ones. What lives HERE is the part that is a decision rather than a
// query — which column wins where a row and its manifest could disagree, what a
// cursor means, and what a row that cannot be serialised means.
type Postgres struct {
	q Querier
}

// Querier is the generated read path, named here as an interface so `Postgres`
// can be built over something else in a test and so the set of statements this
// type can possibly issue is four lines long and visible. `*gen.Queries`
// satisfies it, and nothing else does unless somebody writes it.
type Querier interface {
	ListServices(ctx context.Context, params gen.ListServicesParams) ([]gen.ListServicesRow, error)
	GetService(ctx context.Context, name string) (gen.GetServiceRow, error)
	CountServices(ctx context.Context) (int64, error)
	RequirementsForService(ctx context.Context, serviceName string) ([]gen.RequirementsForServiceRow, error)
	RequiredByForService(ctx context.Context, serviceName string) ([]gen.RequiredByForServiceRow, error)
}

// New builds the catalog over a generated querier.
//
// `*gen.Queries` satisfies `Querier`; the interface is here rather than the
// concrete type so a handler test can mount the real catalog over four canned
// answers without a database.
func New(q Querier) *Postgres { return &Postgres{q: q} }

// List returns the registry, filtered and paged.
//
// THE ORDER OF THE THREE ANSWERS matters and is not incidental:
//
//   - a filter the document cannot answer is ErrBadFilter, before any query
//     runs, because running the query and then refusing the answer would be a
//     round trip spent on a request that was never going to be answered;
//   - a database that cannot be reached is whatever the driver says, and the
//     handler turns it into the 503. There is no retry here and no fallback to an
//     empty page: a retry against a database that is already failing multiplies
//     the load, and an empty page in place of an error is the exact confusion
//     this packet exists to remove;
//   - a filter that matches nothing is an empty `services`, not an error. The
//     collection exists, the question was a good one, and `data: []` says so.
func (p *Postgres) List(ctx context.Context, filter Filter) ([]api.Service, *string, error) {
	if err := filter.Validate(); err != nil {
		return nil, nil, err
	}

	after, err := resolveCursor(filter.Cursor)
	if err != nil {
		return nil, nil, err
	}

	pageSize := filter.PageSize()
	rows, err := p.q.ListServices(ctx, gen.ListServicesParams{
		// One more than asked for. `has_more` is then a fact rather than a second
		// query: a `has_more` computed by counting separately is a statement that
		// can disagree with this one under a concurrent ingest.
		PageSize:  int32(pageSize + 1),
		Kind:      text(filter.Kind),
		Language:  text(filter.Language),
		AfterName: text(after),
		Contract:  text(filter.Contract),
	})
	if err != nil {
		return nil, nil, fmt.Errorf("catalog: listing services: %w", err)
	}

	hasMore := len(rows) > pageSize
	if hasMore {
		rows = rows[:pageSize]
	}

	services := make([]api.Service, 0, len(rows))
	for _, row := range rows {
		svc, err := project(row)
		if err != nil {
			return nil, nil, err
		}
		services = append(services, svc)
	}

	if !hasMore {
		return services, nil, nil
	}
	next := encodeCursor(rows[len(rows)-1].Name)
	return services, &next, nil
}

// Get returns one service, or ErrNoSuchService.
//
// The 404 is for a name that is not there AND for a name whose row exists but is
// not visible to this role, and those two are deliberately the same answer: to a
// public reader a draft is not a service that exists. The distinction is drawn by
// RLS underneath the query, not by a predicate here — see `queries/catalog.sql`.
func (p *Postgres) Get(ctx context.Context, name string) (api.Service, error) {
	row, err := p.q.GetService(ctx, name)
	if errors.Is(err, pgx.ErrNoRows) {
		return api.Service{}, fmt.Errorf("%w: %q", ErrNoSuchService, name)
	}
	if err != nil {
		return api.Service{}, fmt.Errorf("catalog: reading service %q: %w", name, err)
	}
	svc, err := project(asListRow(row))
	if err != nil {
		return api.Service{}, err
	}
	return svc, nil
}

// Count returns how many services this role can see.
//
// The int conversion is the one place a narrowing happens, and it is guarded
// rather than assumed: `count(*)` is a bigint and a registry with more than
// MaxInt32 services is not a thing anybody has, but a truncated count on
// `/readyz` would be a wrong number reported as a fact.
func (p *Postgres) Count(ctx context.Context) (int, error) {
	n, err := p.q.CountServices(ctx)
	if err != nil {
		return 0, fmt.Errorf("catalog: counting services: %w", err)
	}
	if n < 0 {
		return 0, fmt.Errorf("catalog: count(*) returned %d", n)
	}
	return int(n), nil
}

// Requirement is one `requires` edge out of a service.
//
// It is the compatibility graph's answer to "what do I need to run this?", which
// is the question the registry exists to answer and the one Docker Hub, npm and
// GitHub Topics do not: they answer what is SIMILAR, and similarity is not
// edge is one row of the compatibility graph, already projected into the
// document's `CompatibilityEdge`. There is no pantry-owned edge type: the graph
// is on the wire, so the catalog speaks the document's shape and a handler
// cannot serialise a field the document does not declare.
//
// `version_range` is stored verbatim and re-rendered by nothing — a range the
// publisher wrote is a claim about what they tested against, and rewriting it
// into an equivalent form would be the registry quietly editing the claim.

// Requirements returns what one service requires — the FORWARD direction of the
// graph, "what do I need to run this?" — over `requires` edges only.
//
// `conflicts_with` is deliberately unreachable from here. It is a different
// fact ("what must I NOT run alongside?") and answering both under one name is
// how a caller ends up installing a conflict; when it goes on the wire it gets
// its own operation, and the catalog will grow a third method rather than a
// `kind` parameter.
//
// Visibility is the database's answer, not a filter here. 00007's policy makes
// an edge visible exactly when BOTH endpoints are visible to the reading role,
// and the query re-states the target half only because the join would otherwise
// leak it; a draft target hides the edge at the policy, which is CHECK C4's
// assertion. Rows this function returns can therefore be fewer than the table
// holds, and that is the boundary working rather than a bug in the query.
// THE SUBJECT IS CHECKED FIRST, through Get, and that is a decision about what
// "exists" means rather than about round trips: the edge query answers zero
// rows identically for "a leaf of the graph" and for "no such service", and a
// caller who typed a name wrong must be TOLD so (404) rather than handed an
// empty graph to install from (200). Reusing Get means this interface has one
// definition of existence, the same one the service route uses — a second
// query that answers the same question is a second answer that can disagree
// with the first.
func (p *Postgres) Requirements(ctx context.Context, name string) ([]api.CompatibilityEdge, error) {
	if _, err := p.Get(ctx, name); err != nil {
		return nil, err
	}
	rows, err := p.q.RequirementsForService(ctx, name)
	if err != nil {
		return nil, fmt.Errorf("catalog: reading requirements of %q: %w", name, err)
	}
	out := make([]api.CompatibilityEdge, 0, len(rows))
	for _, r := range rows {
		out = append(out, api.CompatibilityEdge{
			Name:         r.TargetName,
			VersionRange: r.VersionRange.String,
			Dependency:   api.CompatibilityEdgeDependency(r.Dependency),
		})
	}
	return out, nil
}

// RequiredBy returns what depends on one service — the BACKWARD direction,
// "who breaks if I change this?" — over `requires` edges only.
//
// The mirror of `Requirements` with the visibility predicate on the other
// endpoint: the subject is `target_id`, the other endpoint is the requirer, and
// an edge FROM a draft service is hidden here exactly as an edge INTO a draft
// is hidden in the forward direction. The consequence is named in the document
// because it is real: a service's public backward graph grows as work in
// progress is published, and that is the conservative answer — nothing claims a
// dependency the caller cannot also read the other side of.
// Same subject-first check as `Requirements`, for the same reason: the backward
// query cannot distinguish a leaf ("nothing requires caf") from a name that was
// never registered, and the 404 belongs to the second and only the second.
func (p *Postgres) RequiredBy(ctx context.Context, name string) ([]api.CompatibilityEdge, error) {
	if _, err := p.Get(ctx, name); err != nil {
		return nil, err
	}
	rows, err := p.q.RequiredByForService(ctx, name)
	if err != nil {
		return nil, fmt.Errorf("catalog: reading the services that require %q: %w", name, err)
	}
	out := make([]api.CompatibilityEdge, 0, len(rows))
	for _, r := range rows {
		out = append(out, api.CompatibilityEdge{
			Name:         r.RequirerName,
			VersionRange: r.VersionRange.String,
			Dependency:   api.CompatibilityEdgeDependency(r.Dependency),
		})
	}
	return out, nil
}

// project turns a row into the document's `Service`.
//
// # WHICH SOURCE WINS, AND WHY IT IS THE MANIFEST
//
// The document's own provenance table says it: `name`, `description`, `language`,
// `core`, `exposes`, `consumes`, `dependencies`, `repository` and `owner` come
// from the service's `cafaye.yml`, and only `kind` and `basePath` are registry
// facts. So the manifest is decoded into the whole object and the two columns the
// manifest cannot state are written over it.
//
// The columns DO carry duplicates — `services.language` and
// `services.core_constraint` are there to be indexed and filtered on — and this
// function deliberately reads those from the manifest rather than from the column.
// The consequence is named here because it is real: nothing in the schema
// constrains `services.language` to equal `manifest->>'language'`, so a row whose
// ingest wrote them differently would be filtered by one value and described by
// another. Constraining a jsonb key to a sibling column is a CHECK on the next
// migration, and it belongs with the ingest rather than with a read path.
//
// # A ROW THAT CANNOT BE SERIALISED IS AN ERROR
//
// The document marks `name`, `language`, `kind`, `core`, `basePath`, `exposes`,
// `consumes`, `dependencies`, `repository` and `owner` as REQUIRED. A manifest
// missing one of them would otherwise serialise as `""` or `null` on a key the
// contract says is always present — a 200 whose body lies about the row's shape,
// and a client that caches it. So the projection checks and refuses, naming the
// row, and the handler turns that into a 500: the database is reachable and has
// answered, and what it answered is not a service.
func project(row gen.ListServicesRow) (api.Service, error) {
	var svc api.Service
	if err := json.Unmarshal(row.Manifest, &svc); err != nil {
		return api.Service{}, fmt.Errorf("catalog: service %q has a manifest this service cannot "+
			"read as JSON (%v); refusing rather than publishing a half-filled object", row.Name, err)
	}

	// The row's NAME is the key the rest of this response is addressed by, and the
	// manifest's name is a copy of it. If they disagree the caller asked for one
	// and would be handed the other, so the row wins and the disagreement is an
	// error rather than a silent substitution.
	svc.Name = row.Name

	svc.Kind = api.ServiceKind(row.Kind)
	if !knownKind(svc.Kind) {
		return api.Service{}, fmt.Errorf("catalog: service %q has kind %q, which the document's "+
			"ServiceKind vocabulary does not contain", row.Name, row.Kind)
	}

	svc.BasePath = nil
	if row.BasePath.Valid {
		bp := row.BasePath.String
		svc.BasePath = &bp
	}

	// `data` is always an array and `consumes`/`dependencies` are "empty rather
	// than absent". A manifest that omits them decodes to a nil slice, which
	// serialises as `null`, and `null` is not what the document publishes for an
	// empty list. Normalised here rather than in the schema, because it is a
	// property of the RESPONSE and not of the row.
	if svc.Consumes == nil {
		svc.Consumes = []string{}
	}
	if svc.Dependencies == nil {
		svc.Dependencies = []api.Dependency{}
	}

	switch {
	case svc.Language == "":
		return api.Service{}, missing(row.Name, "language")
	case svc.Core == "":
		return api.Service{}, missing(row.Name, "core")
	case svc.Repository.URL == "":
		return api.Service{}, missing(row.Name, "repository.url")
	case svc.Owner.Team == "":
		return api.Service{}, missing(row.Name, "owner.team")
	}
	return svc, nil
}

func missing(name, key string) error {
	return fmt.Errorf("catalog: service %q has no %q in its manifest, and the document marks it "+
		"required; a row the response cannot serialise is an error, not an empty object", name, key)
}

// asListRow is the one place `GetServiceRow` becomes a `ListServicesRow`.
//
// sqlc emits a struct PER NAMED QUERY, so two statements that select the same seven
// columns produce two types with the same fields. Converting in one named function
// is what keeps that from becoming two hand-written projections that can drift —
// and it will need editing if a seventh column is added to either statement, which
// is the correct time to find out.
func asListRow(r gen.GetServiceRow) gen.ListServicesRow {
	return gen.ListServicesRow{
		Name:           r.Name,
		Description:    r.Description,
		Language:       r.Language,
		Kind:           r.Kind,
		CoreConstraint: r.CoreConstraint,
		BasePath:       r.BasePath,
		Manifest:       r.Manifest,
	}
}

// text renders an optional generated enum as the nullable TEXT the query binds.
// A nil pointer is SQL NULL, which is how a filter that was not asked for becomes
// "this predicate does not apply" rather than "this predicate is false" — the
// difference between `?kind=api` and no `kind` at all.
func text[T ~string](v *T) pgtype.Text {
	if v == nil {
		return pgtype.Text{}
	}
	return pgtype.Text{String: string(*v), Valid: true}
}

// wrapBadFilter keeps `errors.Is(err, ErrBadFilter)` true while the message says
// which parameter was wrong and what was allowed — which is what the document asks
// a 400 to name, and which is invisible to a client that only switches on the code.
func wrapBadFilter(msg string) error { return fmt.Errorf("%w: %s", ErrBadFilter, msg) }

// ---------------------------------------------------------------------------
// THE CURSOR
// ---------------------------------------------------------------------------

// cursorVersion prefixes every cursor this encoding issues. Its job is to make
// "a cursor this service did not issue" a checkable statement rather than a hope:
// a base64 blob that decodes to something that is not `1:<a legal service name>`
// was not issued here, and the document says that is a 400 — "never silently page
// one, which loses rows without saying so".
const cursorVersion = "1:"

// encodeCursor renders the name of the last row on a page.
//
// It is base64url with no padding, which is what the document asks for, and it is
// NOT signed. A signature would make "a cursor this service did not issue"
// literally true; without one the guarantee is the weaker and stated one — an
// unparseable or out-of-version cursor is refused, so a client cannot silently
// restart at page one and lose rows. Signing it would also mean a key in the
// process, a rotation story, and a second thing that can be wrong, for a cursor
// over data that only a reviewed ingest changes.
func encodeCursor(name string) string {
	return base64.RawURLEncoding.EncodeToString([]byte(cursorVersion + name))
}

// decodeCursor returns the name to resume after, or ErrBadCursor.
func resolveCursor(cursor *string) (*string, error) {
	if cursor == nil {
		return nil, nil
	}
	raw, err := base64.RawURLEncoding.DecodeString(*cursor)
	if err != nil {
		return nil, badCursor(*cursor, "it is not base64url")
	}
	name, ok := strings.CutPrefix(string(raw), cursorVersion)
	if !ok {
		return nil, badCursor(*cursor, "it was not issued by this encoding")
	}
	if !serviceNameGrammar.MatchString(name) {
		return nil, badCursor(*cursor, "it does not name a service this registry could hold")
	}
	return &name, nil
}

func badCursor(got, why string) error {
	return wrapBadFilter("cursor is unusable: " + why + ". got " + quote(got) +
		". A cursor must be a page.next_cursor from an earlier response to this endpoint, " +
		"and a cursor this service did not issue is refused rather than silently read as page one")
}

func quote(s string) string {
	const max = 64
	if len(s) <= max {
		return "\"" + s + "\""
	}
	return "\"" + s[:max] + "…\" (" + itoa(len(s)) + " bytes)"
}
