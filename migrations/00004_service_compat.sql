-- 00004_service_compat.sql — THE COMPATIBILITY GRAPH.
--
-- Read this header before changing anything in it. This table is the moat: it
-- answers "which services can I run that compose with what I already run?", which
-- is a question Docker Hub, npm and GitHub Topics do not answer — they answer
-- "what is similar", and similarity is not composition. It is a FIRST-CLASS TABLE
-- from day one and not a JSON column someone queries later, because a JSON column
-- added later is a migration run against every existing row and it constrains
-- nothing about how the rows above it are modelled. This table constrains
-- `services` (a foreign key in both directions is a constraint on what a service
-- may depend on), it is indexed in both directions, and its CHECKs are the rules.
--
-- ONE ROW IS ONE EDGE: service A requires service B, over a version range. The
-- edge is DIRECTIONAL and both directions are indexed because the two questions
-- are different and one of them is the product question:
--
--   forward   "what do I need to run this?"        services -> requires
--   backward  "what breaks if I upgrade this?"     required_by <- services
--
-- The backward direction is the one the Docker Hub search box does not have and
-- the reason the table is worth building early: an operator holding six running
-- services asking "which of my services would be affected if I upgrade this one"
-- is the query that decides whether they upgrade.
--
-- WHY THE RANGE IS TEXT AND NOT A RANGE TYPE. core's grammar is four forms —
-- `^1.2.3` (caret), `~1.2.3` (tilde), `>=1.2.3` (gte), and a bare `1.2.3` which
-- is exact. That is core's published vocabulary (MVP-SCOPE, "checked against
-- core's published versions at write time"), and re-expressing it as a native
-- range type would mean a migration every time core adds a fifth form, plus a
-- lossy conversion of the four that exist. The string is stored verbatim; the
-- CHECK below is what makes a string in this column mean something, and it is
-- written to accept exactly core's four forms and nothing else.
--
-- THE CHECK IS THE ONLY VALIDATION THAT MATTERS HERE, and it is a CHECK rather
-- than application code because the application is six months of churn away from
-- being the thing you want to trust with a range grammar.

-- +goose Up

create table pantry.service_compat (
  -- The composite primary key is the edge. `(service_id, target_id, kind)` rather
  -- than a surrogate id because the edge IS the entity: there is no third fact
  -- about an edge that is not an attribute of it, and a surrogate id would be a
  -- column that a delete has to remember to clean up.
  --
  -- `kind` is in the key because the same pair can appear twice with a different
  -- MEANING — `requires` is a hard dependency and `conflicts_with` is a
  -- documented incompatibility — and a key without it would refuse the second
  -- fact rather than store it.
  service_id   uuid not null references pantry.services (id) on delete cascade,
  target_id    uuid not null references pantry.services (id) on delete cascade,
  kind         text not null,

  -- THE RANGE. core's four-form grammar, verbatim, NULL only for
  -- `conflicts_with` where a range would be meaningless.
  --
  -- The grammar, spelled out, because the pattern is the specification and a
  -- reader should not have to go find core to know what is accepted:
  --
  --   ^1.2.3    caret. Pre-1.0 a caret pins the MINOR (core's rule, and the reason
  --              `registry/index.yml` records `^0.1.0` as excluding `^0.2.0`), so
  --              ^0.1.0 admits >=0.1.0 <0.2.0. At or above 1.0 it admits >=1.2.3
  --              <2.0.0.
  --   ~1.2.3    tilde. >=1.2.3 <1.3.0 — the patch level may move, the minor may
  --              not.
  --   >=1.2.3   gte, unbounded above.
  --   1.2.3     exact, because core's four-form grammar treats a bare triple as
  --              itself and not as a shorthand for `=`.
  --
  -- No leading `v`, no wildcards, no spaces, no ranges like `>=1.0.0 <2.0.0`. Each
  -- is refused by name in the error rather than silently normalised, because a
  -- registry that quietly rewrites a publisher's constraint is publishing
  -- something the publisher did not write.
  version_range text,

  -- WHY A THIRD STATE BESIDES `required`/`optional`. muse's manifest carries
  -- `required: false` for guard and a comment saying the reason — muse runs
  -- degraded without it — and that is a *soft dependency*: the service starts
  -- without it and loses a capability. "Optional" in the general sense would also
  -- cover "we would like this if it is there", which is not the same promise, and
  -- the compatibility graph's whole value is that a caller can trust the answer.
  dependency   text not null default 'required',

  -- WHERE THE EDGE CAME FROM. A first-party edge is read off a manifest by the
  -- ingest; a third-party edge is what the publisher typed. Both are publisher-
  -- declared in the sense MVP-SCOPE means ("the registry is excellent rendering
  -- of publisher-declared metadata, not an attestation") and the difference is
  -- recorded because it is the difference between a reviewed claim and an
  -- unreviewed one, and a reader of the API needs to know which they are looking
  -- at.
  source       text not null default 'manifest',

  -- The manifest the edge was read from, when source = 'manifest'. NULL for a
  -- publisher-declared edge. Kept for the same reason `services.manifest_sha256`
  -- is kept: so an ingest failure can be diffed against the bytes it came from.
  declared_in  text,

  created_at   timestamptz not null default now(),

  -- NO SELF-EDGES. A service requiring itself is either a typo or a cycle of
  -- length one, and `00005`'s traversal query would treat it as a loop forever.
  constraint service_compat_no_self_edge check (service_id <> target_id),

  -- THE GRAMMAR. `version_range` is required exactly for the two kinds that have
  -- a version meaning, and refused for the kind that does not — the constraint is
  -- stated as an equivalence rather than as two half-checks because a half-check
  -- is satisfied by BOTH nulls.
  constraint service_compat_range_required_iff_versioned
    check ((kind in ('requires', 'conflicts_with')) = (version_range is not null)),

  -- THE FOUR FORMS, AND NOTHING ELSE. Written as a positive pattern over the
  -- whole string rather than as a chain of negations, so a fifth form added to
  -- core is refused here by name until somebody decides how this table stores it.
  constraint service_compat_range_is_core_grammar
    check (version_range is null
        or version_range ~ '^(\^|~|>=)?[0-9]+\.[0-9]+\.[0-9]+$'),

  -- dependency, and NOT NULL with a check rather than an enum, for the reason
  -- the language check on `services.language` is: core's vocabulary is the
  -- authority and adding a value must not be a type migration. The three values
  -- are required / soft / dev, and the third exists because a graph that cannot
  -- say "this edge is for the dev stack" makes every local-compose tool
  -- reimplement the concept.
  constraint service_compat_dependency_is_known
    check (dependency in ('required', 'soft', 'dev')),

  -- The same vocabulary for the edge's KIND, and the reason the key holds kind:
  -- `requires` and `conflicts_with` are the two directions a compatibility
  -- question can take and both are first-class.
  constraint service_compat_kind_is_known
    check (kind in ('requires', 'conflicts_with')),

  -- The edge, as a constraint rather than as documentation. `(service_id,
  -- target_id, kind)` is the primary key, so this names the property the key
  -- already has; it exists because the primary key could be dropped and
  -- re-added as a plain unique, and the graph's indexing assumption is
  -- uniqueness, not uniqueness-under-a-name.
  constraint service_compat_edge_unique unique (service_id, target_id, kind)
);

-- =============================================================================
-- THE TWO DIRECTION INDEXES. This is the part the brief is explicit about, and
-- the reason it is worth writing down rather than letting the planner discover.
-- =============================================================================

-- BACKWARD: "who requires this?" — the upgrade-safety question, and the one the
-- product exists to answer. `target_id` leads, `kind` second so the `requires`
-- and `conflicts_with` edges into one service are two adjacent ranges rather than
-- one mixed scan.
--
-- It is a FULL index rather than a partial one, unlike the catalog's published-row
-- indexes, and the asymmetry is deliberate: an operator asking what a version
-- upgrade breaks needs to see the edges into a DRAFT service too, because that
-- draft is somebody's next release and "upgrading to it would break four
-- services" is exactly the answer they need before it is public.
create index service_compat_target_idx
  on pantry.service_compat (target_id, kind, service_id);

-- FORWARD: "what does this need?" — the install question. `service_id` leads and
-- `kind` is second for the same reason as above, so a single edge's metadata comes
-- back from one contiguous range rather than from a filter over the whole table.
--
-- The primary key already indexes (service_id, target_id, kind), which is why this
-- is not redundant: the primary key's third column is `target_id`, so a query that
-- filters on `service_id` and `kind` and sorts by `target_id` cannot use it as an
-- index scan — it would have to filter `kind` across the whole service's edges.
-- This index is the one that makes `kind` a prefix, which is what a read of "this
-- service's requires edges, excluding its conflicts" actually is.
create index service_compat_service_idx
  on pantry.service_compat (service_id, kind, target_id);

-- THE RECIPROCAL EDGE IS NOT CONSTRAINED, and the omission is a decision rather
-- than a gap. `requires` is a directed edge: if A requires B, it is not the case
-- that B requires A, and asserting symmetry would forbid the single most common
-- real relationship in the fleet (muse requires identity; identity requires
-- nothing). What IS worth asserting is that a conflict is symmetric, and that
-- check cannot be written as a table constraint in Postgres without a trigger —
-- a subquery in a CHECK is not allowed. It is therefore NOT asserted here, and
-- this comment is where that is recorded rather than in a report somebody has to
-- find: `00006`'s isolation suite covers the traversal, and a symmetric-conflict
-- assertion is the one known hole, named.
--
-- THE CYCLE QUESTION IS ALSO UNANSWERED HERE, for the same structural reason. A
-- requires-cycle (A needs B, B needs A) is not an integrity error — it is a real
-- deployment topology and `caf dev` may well want to express it — but it does mean
-- any recursive traversal needs a visited-set. That belongs in the traversal query
-- in `00006`, not in a constraint that would reject a legitimate graph.

-- +goose Down

-- Two foreign keys to the same table, and DROP TABLE handles both. The explicit
-- drops are here for the same reason as everywhere else in this directory: a Down
-- that assumes its own Up ran must not be the thing that fails on a partially
-- applied database.
alter table if exists pantry.service_compat
  drop constraint if exists service_compat_edge_unique;
alter table if exists pantry.service_compat
  drop constraint if exists service_compat_no_self_edge;

drop table if exists pantry.service_compat;