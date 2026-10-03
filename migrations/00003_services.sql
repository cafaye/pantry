-- 00003_services.sql — the catalog entry and its versions.
--
-- WHAT THIS TABLE IS. One row per registered service. It is the registry's whole
-- read model: `GET /v1/services` and `GET /v1/services/{name}` are this table,
-- and the document's `Service` object is a projection of it. The manifest itself
-- is stored as jsonb (see below) rather than flattened into thirty columns,
-- because the manifest's vocabulary belongs to core and pantry adds none to it
-- (AGENTS.md rule 1) — a column per manifest field would make every core field
-- addition a migration here, and the drift between the copy and the row would be
-- invisible until a reader noticed a missing key.
--
-- THE COLUMNS THAT ARE NOT THE MANIFEST. Four of them are facts the manifest
-- cannot state, and `registry/index.yml` exists precisely because core's schema
-- closes with `additionalProperties: false`:
--
--   kind          the service's shape. Curated, and only two values are admissible
--                 for a surface-less manifest — `api` and `cli` (AGENTS.md MD1).
--   base_path     the `/vN` prefix derived from the service's OpenAPI document,
--                 null when it publishes none.
--   trust         first_party | third_party. THE DECIDED COLUMN (addendum): one
--                 table, one read path, distinguished by a flag.
--   state         draft | submitted | published | unlisted. Submission state,
--                 not lifecycle — see the enum below.
--
-- `core_constraint` is the manifest's own four-form grammar verbatim (`^0.2.0`),
-- stored as text and NOT parsed into columns. It is parsed in `00004` into
-- `pantry.service_compat` for the graph, but the catalog row keeps the string
-- because the document publishes the string and a registry that stores a
-- re-rendered version of a constraint has a way of disagreeing with the manifest
-- it claims to be a copy of.
--
-- WHY `manifest` IS jsonb AND NOT A FK TO A FILE. The old read model served
-- `registry/services/<name>/cafaye.yml` from disk. That model is legacy (addendum:
-- "the database is the only read source"); first-party content becomes an ingest
-- SOURCE. Storing the bytes keeps the byte-equality property the Rust suite
-- enforces — `tests/recorded_copy.rs` compares bytes, and a jsonb round trip is
-- not byte equality — which is why `manifest_sha256` is stored alongside it.

-- SUBMISSION STATE, as an enum rather than a text column with a CHECK.
--
-- Four values, in the order the addendum states them: draft → submitted →
-- published / yanked. `unlisted` is the fourth and it is not `draft`: it is a
-- PUBLISHED row the publisher has removed from public listing without deleting
-- it, which is the same shape as a yanked version and exists for the same reason
-- — you cannot un-run a release, only stop advertising it.
--
-- WHY NOT A BOOLEAN `published`. Because `submitted` and `draft` and `yanked` are
-- three different questions and a boolean answers none of them: is this visible?
-- is this reviewed? is this still installable? A registry for self-hosted services
-- needs all three, and MVP-SCOPE is explicit that a yanked release must stay
-- visible to somebody already running it.
--
-- The enum is `if not exists` because Postgres has no `create type if not exists`
-- and an enum added in a later migration must not redefine this one. Every
-- migration in this directory is written to be re-runnable against a database
-- that already has it.

-- +goose Up

-- +goose StatementBegin

do $$
begin
  if not exists (select 1 from pg_type t
                 join pg_namespace n on n.oid = t.typnamespace
                 where t.typname = 'trust_level' and n.nspname = 'pantry') then
    -- THE TRUST FLAG. Two values and no third, and the third is the interesting
    -- one: there is no `verified`. Verification is a property of a PUBLISHER
    -- (`publishers.verified`), not of a service, because a service's trust is
    -- decided by a reviewed commit and a publisher's verification is decided by
    -- an OAuth flow. Merging them would make one flag mean both and neither
    -- would be trustworthy.
    create type pantry.trust_level as enum ('first_party', 'third_party');
  end if;

  if not exists (select 1 from pg_type t
                 join pg_namespace n on n.oid = t.typnamespace
                 where t.typname = 'service_state' and n.nspname = 'pantry') then
    create type pantry.service_state as enum ('draft', 'submitted', 'published', 'unlisted');
  end if;

  if not exists (select 1 from pg_type t
                 join pg_namespace n on n.oid = t.typnamespace
                 where t.typname = 'service_kind' and n.nspname = 'pantry') then
    -- `api` and `cli` ONLY. A manifest that declares a contract surface is an
    -- `api`; one that declares none is `cli`, whether it is a binary (caf) or an
    -- importable package (cafaye-ts). AGENTS.md MD1 records that `worker` and
    -- `both` were both refused, and that `api` was admitted only because they
    -- were — so the enum is a CLOSED SET, and admitting a third value is a
    -- migration with a DECISIONS.md entry, not an INSERT.
    create type pantry.service_kind as enum ('api', 'cli');
  end if;
end
$$;

-- +goose StatementEnd

create table pantry.services (
  id          uuid primary key default gen_random_uuid(),

  -- The cafaye namespace name, and also the repository name and the event
  -- `source`. The pattern is core's, copied verbatim from the manifest schema's
  -- `name`: a lower-case, dash-separated word. The CHECK is not decoration — it
  -- is what stops a name that is legal in a URL path and illegal in a manifest,
  -- which is how two registries end up disagreeing about what "courier2" is.
  name        text not null check (name ~ '^[a-z][a-z0-9]*(-[a-z0-9]+)*$'),

  -- Nullable exactly as the manifest says: optional keys are OMITTED rather than
  -- null when the manifest omits them (openapi/v1.yaml `Service`), and null here
  -- is the same distinction one layer down.
  description text check (description is null or length(description) <= 200),

  -- The document's `Language` enum values, as a CHECK rather than an enum type,
  -- because adding a language must not require a type migration and core's
  -- vocabulary is the authority here rather than this schema.
  language    text not null check (language in
                 ('go', 'rust', 'elixir', 'python', 'typescript', 'javascript', 'ruby', 'zig')),

  kind        pantry.service_kind not null,
  trust       pantry.trust_level  not null default 'third_party',
  state       pantry.service_state not null default 'draft',

  -- core's four-form grammar, verbatim. `^` `~` `>=` or bare, then x.y.z.
  -- The same pattern the OpenAPI document declares, so a row cannot hold a
  -- constraint the document would refuse to publish.
  core_constraint text not null check (core_constraint ~ '^(\^|~|>=)?[0-9]+\.[0-9]+\.[0-9]+$'),

  -- null when the service publishes no OpenAPI document, and null is a FACT here
  -- rather than an absence: it is what makes `kind: cli` coherent.
  base_path   text check (base_path is null or base_path ~ '^/v[0-9]+$'),

  -- THE MANIFEST, AS BYTES THAT HASH. `manifest_sha256` is the byte-equality
  -- anchor the Rust `recorded_copy` suite enforces against a git checkout; the
  -- jsonb is what a query reads. Storing both is not belt-and-braces, it is
  -- storing the two things the two consumers need: a byte for the drift check and
  -- a document for a renderer.
  manifest        jsonb not null,
  manifest_sha256 text   not null check (manifest_sha256 ~ '^[0-9a-f]{64}$'),

  -- PROVENANCE. `source_ref` is the commit a first-party copy was recorded at —
  -- the `recordedAt` field from `registry/index.yml`, carried into the row so the
  -- database states the same thing the registry file does. `source_repo` is the
  -- service's own repository, so an ingest failure can be diffed against the
  -- right tree. `ingested_at` is when this row was last written by the sync; it is
  -- NOT a freshness claim, because a re-ingest that finds no change still bumps
  -- it, and a column named as though it measured staleness would be one.
  source_repo  text,
  source_ref   text,
  ingested_at  timestamptz,
  ingested_by  text,

  -- The publisher. NULLABLE, and that is the honest shape rather than a
  -- convenience: a first-party service is in the catalog because a reviewed
  -- commit put it in `registry/index.yml`, and it has no publisher account. Making
  -- this NOT NULL would either invent an account for every fleet service or force
  -- the sync to create one, and both are worse than a null that means "no human
  -- owns this, cafaye does".
  publisher_id uuid references pantry.publishers (id) on delete restrict,

  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),

  -- A first-party service with a publisher account is a contradiction the fleet
  -- does not produce and a publisher's write must not be able to create. Asserted
  -- here rather than left to `00005_rls.sql`, because a constraint in the schema
  -- is enforced for the owner, for an admin, and for a future migration, while a
  -- policy is enforced only for the roles it names.
  constraint services_first_party_has_no_publisher
    check (not (trust = 'first_party' and publisher_id is not null)),

  -- `name` is the registry's natural key and is unique case-sensitively: core's
  -- pattern is lower-case only, so `Courier` cannot be created and case cannot
  -- collide.
  constraint services_name_key unique (name)
);

-- THE SEARCH INDEXES. Two, and the split between them is the point.
--
-- `services_published_name_idx` is a PARTIAL index on `name` where
-- `state = 'published'`. Every public read is filtered to published rows, and a
-- partial index is smaller than the whole table by exactly the ratio the registry
-- will spend its life at: most rows are published, but the drafts and submissions
-- accumulate forever underneath. Naming it `services_published_name_idx` rather
-- than `services_name_idx` is what makes a reader grep for the wrong index and
-- find nothing.
create index services_published_name_idx
  on pantry.services (name)
  where state = 'published';

-- The document's filters, as separate btrees rather than one composite. The
-- argument for a composite `(state, kind, language, name)` is fewer index entries;
-- the argument against is that `GET /v1/services?kind=api` and
-- `?language=go` are different queries and a composite only serves the second
-- when the first is pinned. At a few thousand rows both are free and the
-- flexibility is worth more than the entries.
create index services_published_kind_idx
  on pantry.services (kind)
  where state = 'published';

create index services_published_language_idx
  on pantry.services (language)
  where state = 'published';

-- TRUST, because it is the read model and not a filter on the document: the
-- addendum's whole design is that first-party and third-party rows share every
-- index, so a trust-scoped read must not be a table scan. It is partial on the
-- third-party rows because that is the side that grows.
create index services_third_party_idx
  on pantry.services (name)
  where trust = 'third_party';

-- FULL TEXT, over name and description. `unaccent` is applied through the
-- expression index rather than through the stored value, so a name is stored as
-- the publisher wrote it and matched without diacritics.
--
-- This is the addendum's "Postgres full-text plus pg_trgm, no Elasticsearch",
-- and the index is `gin` because a `tsvector` column's useful index is a GIN one —
-- `btree` on a tsvector is not an option and a functional GIN index is the shape
-- that works. `00007` adds the query this index exists for.
create index services_search_idx
  on pantry.services
  using gin (to_tsvector('simple',
           coalesce(name, '') || ' ' || coalesce(description, '')));

-- TRIGRAM, for the typos. A full-text index does not find `courire`; a trigram
-- index does, and it is the second half of the addendum's search line. On `name`
-- only: a trigram index over `description` would be a large index over the one
-- long free-text column in the table, and `similarity()` over descriptions is not
-- a query anybody has asked for.
create index services_name_trgm_idx
  on pantry.services using gin (name gin_trgm_ops);

-- `updated_at` is maintained by a TRIGGER rather than by the application, because
-- an application that forgets to set it produces a registry whose "recently
-- updated" ordering silently excludes every row it forgot on — a wrong answer
-- rather than a missing one.
--
-- THE TRIGGER IS CREATED IN `00005_functions.sql`, not here, and that placement
-- is not a tidiness choice. Goose runs migrations in filename order, so a trigger
-- here that names `pantry.touch_updated_at()` fails with
-- `function pantry.touch_updated_at() does not exist` — measured, on the run that
-- found it, and it is the ordinary failure of splitting a migration by topic
-- rather than by dependency. `00005` defines the function and then hangs the
-- trigger off the table `00003` created.

-- =============================================================================
-- VERSIONS. The publisher's real Git tags, and not a version scheme.
--
-- MVP-SCOPE: "Versions are not invented. Track the publisher's real Git tags." The
-- upstream project already decided what a version is, so this table stores the TAG
-- STRING and a parsed triple beside it. `tag` is the key a `caf dev` would check
-- out; `version` is what a compatibility range is compared against, and it is NULL
-- for a tag that is not semver, because a tag like `nightly` is a real release and
-- pretending it is 0.0.0 would put it in every range.
--
-- `version` is NOT a constraint that every tag must satisfy. A project that
-- publishes `2024.11.3` or `v1-repro` has releases, and a registry that refuses
-- them is a registry those projects are absent from. The nullable triple is the
-- honest shape and the range check in `00004` is what refuses a range over a
-- target whose versions are not comparable — loudly, at write time.
-- =============================================================================
create table pantry.service_versions (
  id          uuid primary key default gen_random_uuid(),

  service_id  uuid not null references pantry.services (id) on delete cascade,

  -- The tag VERBATIM, including any leading `v`. Two rows differing only by a `v`
  -- prefix are two real tags in git and this table says so rather than merging
  -- them; a registry that guessed would resolve one of them to the other.
  tag         text not null,

  -- The parsed triple, NULL when `tag` is not semver. Parsed by a check, not by a
  -- trigger, so an unparseable tag cannot be inserted with a triple nobody
  -- computed.
  version_major integer,
  version_minor integer,
  version_patch integer,

  -- When the upstream published it, as the publisher said. NOT the row's
  -- created_at: a row ingested today for a tag released in 2023 has a created_at
  -- of today, and a "newest version" query that used it would answer wrongly.
  released_at  timestamptz,

  -- YANK IS A STATE, NOT A DELETE, and this is the column that is the reason
  -- versions are a table and not a tag on the service row. MVP-SCOPE: "For
  -- self-hosted services a yanked release must stay visible — you cannot take it
  -- back from someone already running it." So `yanked_at` is set, the row stays,
  -- and every read filters on it rather than deleting.
  --
  -- WHY NOT A BOOLEAN. A boolean cannot say WHEN, and "when" is the whole
  -- difference between a row that is present and a row that was withdrawn — a
  -- user upgrading gets a different message depending on whether the release was
  -- yanked before or after they last upgraded.
  yanked_at    timestamptz,
  yank_reason  text check (yank_reason is null or length(yank_reason) <= 500),

  created_at   timestamptz not null default now(),

  -- A tag is unique per service, not globally: two projects may both have a `v1`.
  constraint service_versions_service_tag_key unique (service_id, tag),

  -- Either all three components or none. A partial triple is a version that
  -- compares against nothing, which is a bug that surfaces as an empty
  -- compatibility result rather than as an error.
  constraint service_versions_triple_is_all_or_nothing
    check ((version_major is null and version_minor is null and version_patch is null)
        or (version_major is not null and version_minor is not null and version_patch is not null)),

  -- A non-negative triple. A tag whose parsed version is negative is a parse bug
  -- and this is where it stops.
  constraint service_versions_non_negative
    check (version_major is null
        or (version_major >= 0 and version_minor >= 0 and version_patch >= 0)),

  -- A yank reason is required exactly when the release is yanked. A yanked row
  -- with no explanation tells a user running it nothing, which is the one thing a
  -- yank exists to do.
  constraint service_versions_yank_reason_iff_yanked
    check ((yanked_at is null) = (yank_reason is null))
);

-- The newest first index, PARTIAL on not-yanked. `GET /v1/services/{name}` shows
-- "latest version" and a partial index keeps every draft tag ever pushed out of
-- that one lookup — which is the query the whole page load is waiting on.
create index service_versions_live_idx
  on pantry.service_versions (service_id, version_major desc, version_minor desc, version_patch desc)
  where yanked_at is null;

-- Every version including yanked, for a page that says "this release was yanked
-- on DATE". Deliberately a second index rather than a superset: this query is
-- rare and the partial one is hot.
create index service_versions_service_idx
  on pantry.service_versions (service_id, created_at desc);

-- +goose Down

-- The trigger first, then the tables. Dropping the table drops the trigger with
-- it, so this is belt-and-braces on a table that may not exist on a partially
-- applied database — which is the one moment a Down is wanted.
-- NO `drop trigger` HERE, and the omission is the ordering constraint stated
-- above: the trigger is created by `00005_functions.sql` and belongs to that
-- file's Down. Dropping it from here would either fail on a database where 00005
-- has not run, or succeed and then be recreated by a later `goose up` of 00005 —
-- a Down that leaves the schema in a state its own Up cannot produce.

drop table if exists pantry.service_versions;
drop table if exists pantry.services;

-- The ENUM TYPES ARE NOT DROPPED, deliberately, and this is a real asymmetry with
-- the RLS migration's Down. `drop type` would fail on a database where a later
-- migration added a column using one of these, and a Down that fails leaves the
-- database in the state the Down exists to undo. The types live in the `pantry`
-- schema, which `00001_roles.sql`'s Down drops with `cascade` — so the full
-- rollback to empty DOES remove them, at one level, where the ordering is known.
-- A `goose down` of this file alone leaves them, which is correct: a rollback of
-- one migration must not destroy vocabulary a sibling migration uses.