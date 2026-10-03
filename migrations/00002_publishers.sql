-- 00002_publishers.sql — who may write to the registry.
--
-- A publisher is an account, and an account here is exactly one thing: a GitHub
-- identity. The addendum skips "accounts beyond GitHub identity", so this table
-- carries no password hash, no email, no session, no plan and no billing
-- identity. identity owns authentication; this table records which GitHub identity
-- owns which catalog entries, and nothing else about a person.
--
-- WHY A TABLE AND NOT A COLUMN. `services.publisher_id` as a github_user_id would
-- be one column fewer, and it would make "the list of services I publish" a query
-- the database cannot index against a join and would make "rename the owner" a
-- migration. More importantly it would make the trust flag meaningless: the
-- addendum's decision is that first-party and third-party rows live in the SAME
-- tables and are distinguished by a flag, and a row's trust cannot be a property
-- of a foreign key to a table only some rows point at — that is the separate-
-- tables design wearing a disguise.
--
-- `is_first_party` is therefore a COLUMN here and NOT a per-publisher setting, and
-- that is deliberate: a first-party service is a fact about the cafaye fleet's own
-- manifests, decided by a reviewed commit to `registry/index.yml`, and never by
-- anybody who registers an account. It is the one field on this table that no
-- publisher may set, and `00005_rls.sql`'s insert policy refuses the write rather
-- than trusting the application to have left it alone.
--
-- ONE TABLE, THREE COLUMNS THAT ARE NOT DECORATION:
--
--   github_id  the numeric account id. Immutable, and the thing to key on: a
--              login can be renamed, changed, and squatted after release, and a
--              registry keyed on a mutable string is a registry whose ownership
--              can be transferred by a rename. github_login is kept because every
--              human reading a row wants it, and it is unique but not the key.
--   claimed_at when the identity first claimed the name. It is the fact that the
--              publisher was at a real account when they took it, and it is the
--              one value in this table a later row can be checked against after
--              the login has been renamed.
--   verified   whether the OAuth flow proved control of the account. FALSE for a
--              row created by a migration or a fixture, and the honest value for a
--              row the sync wrote for a first-party service. `verified` is NOT an
--              authorisation input in `00005_rls.sql` — a publisher role writes
--              its own rows whether or not this is true — because a policy that
--              branches on a column the publisher can set is a policy the publisher
--              controls. It is a fact to display and to filter on, and saying so
--              here is what stops a later reader from trusting it.

-- +goose Up

create table pantry.publishers (
  -- uuid, not bigint: this id crosses a wire into identity's session claims and
  -- back, and a guessable integer is a guessable publisher id. gen_random_uuid()
  -- is core since Postgres 13, so this needs no extension.
  id          uuid        primary key default gen_random_uuid(),

  -- BIGINT, not text, and not a numeric that could exceed it: GitHub's numeric
  -- account ids fit in int8 comfortably and are stable forever. The check is here
  -- so a bad ingest fails at the boundary rather than becoming a row that cannot
  -- be joined to anything.
  github_id   bigint      not null check (github_id > 0),

  -- A GitHub login is 1-39 chars of [A-Za-z0-9-] and may not be all hyphens.
  -- Matching GitHub's rule exactly rather than loosely is what makes a name from
  -- another site fail here instead of creating a row nothing can verify.
  github_login text       not null check (github_login ~ '^[A-Za-z0-9](?:[A-Za-z0-9]|-(?=[A-Za-z0-9])){0,38}$'),

  -- THE TRUST FLAG. Read the column comment above this file's header: it is set
  -- by the fleet, never by the publisher, and `00005_rls.sql` refuses a publisher
  -- write that sets it.
  is_first_party boolean  not null default false,

  claimed_at  timestamptz not null default now(),
  verified    boolean     not null default false,

  -- CONSTRAINT, NOT JUST AN INDEX. Two accounts may not present the same GitHub
  -- id, and a bare unique index is a constraint — but naming it says what the
  -- property IS, which is what a reader of the table is looking for.
  constraint publishers_github_id_key    unique (github_id),
  constraint publishers_github_login_key unique (github_login)
);

-- The read path is "every publisher, ordered by id" for an admin view and "this
-- publisher" by primary key, so the unique constraints above are the indexes and
-- this one is for the listing. A partial index rather than a full one because the
-- fleet's own accounts are a handful of rows and everything a public page reads is
-- third-party: indexing the third-party rows separately keeps the admin sweep off
-- a table where nearly every page is a third-party page.
create index publishers_third_party_idx
  on pantry.publishers (github_login)
  where not is_first_party;

-- +goose Down

drop table if exists pantry.publishers;