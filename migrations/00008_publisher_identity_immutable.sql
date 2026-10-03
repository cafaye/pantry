-- 00008_publisher_identity_immutable.sql — a publisher may not become a
-- different publisher.
--
-- WHAT WAS BROKEN, MEASURED. `00006`'s `publishers_publisher_update` reads
--
--   using     (id = (select pantry.current_publisher_id()))
--   with check (id = (select pantry.current_publisher_id()))
--
-- and `with check` therefore pins exactly one column: the primary key. Measured
-- on PostgreSQL 18.4, applying 00001..00007 to an empty database and then, as
-- `pantry_publisher` with `begin_publisher('…a1')`:
--
--   update … set github_login    = 'alpha-renamed'  -> UPDATE 1
--   update … set github_id       = 999               -> UPDATE 1   (UNCLAIMED)
--   update … set is_first_party  = true              -> UPDATE 1
--   update … set verified        = false             -> UPDATE 1
--   update … set claimed_at      = '1999-01-01'      -> UPDATE 1
--   update … set github_id       = 202               -> ERROR 23505
--        duplicate key value violates unique constraint "publishers_github_id_key"
--
-- The last line is the only refusal, and it is UNIQUE ARITHMETIC about a value
-- that collides with another row. It is not a policy, it names no rule, and it
-- says nothing about a write aimed at an id nobody holds — which is the write an
-- account-takeover attempt makes. `github_id` is what `00002_publishers.sql:27`
-- calls "the numeric account id. Immutable, and the thing to key on". It was
-- mutable by the one role that owns the row.
--
-- WHY THE FIX IS `with check (false)` AND NOT A CHECK THAT PINS THE COLUMNS.
-- The obvious repair is a `with check` that compares the new `github_id` to the
-- old one. IT CANNOT BE WRITTEN IN POSTGRESQL. A policy may not reference its own
-- relation. Measured on PostgreSQL 18.4 — and the first half of the measurement
-- is the part that makes this a trap: `CREATE POLICY` ACCEPTS such a policy, so
-- the error does not arrive at the statement that wrote it. It arrives on the
-- first statement that uses the table.
--
--   create policy tmp_selfref on pantry.publishers
--     for update to pantry_publisher, pantry
--     using (id = (select pantry.current_publisher_id()))
--     with check (github_id is not distinct from
--       (select p.github_id from pantry.publishers p
--         where p.id = (select pantry.current_publisher_id())));
--     -- accepted, exit 0: CREATE POLICY
--
--   set role pantry_publisher;
--   select pantry.begin_publisher('…a1');
--   update pantry.publishers set github_id = 999 where id = '…a1';
--     ERROR:  42P17: infinite recursion detected in policy for relation "publishers"
--     LOCATION:  fireRIRrules, rewriteHandler.c:2272
--
-- The rewriter raises it, not the executor, so the failure is TOTAL rather than
-- partial: while such a policy exists the role cannot write the table at all, and
-- every statement against it answers 42P17 instead of the write it asked for. A
-- `create policy` that "worked" is therefore not evidence that a policy works,
-- and this file's own `tests/rls.sh` round trip is what distinguishes the two.
--
-- The reference may be laundered through a function, because a function call is
-- opaque to the rewriter — but then the rule stops being visible in `pg_policies`
-- and becomes a `stable` function whose body a reader has to go and find, and the
-- entire argument for a policy is that the rule and the table sit in the same
-- place. So the shape of the fix is not "pin four columns". It is "there is no
-- publisher update at all", which is TRUE, and which one word says.
--
-- WHY THAT IS NOT OVER-REFUSAL. It looks like it pins six columns when the
-- question was about three, so here is the whole justification, and it is not a
-- guess:
--
--   * There is no publisher-shaped write on `publishers` to refuse. DECISIONS.md
--     D31's role map says it in the row for `pantry_publisher`: "a publisher's
--     own rows — submit a service, add a version, add an edge, withdraw …
--     Phase 1 has no self-registration so `publishers` rows come from a
--     migration, a fixture or an admin." Nothing in `internal/` writes to
--     `publishers`; `internal/pantrydb/queries/` is the whole data path and it is
--     reads. The `update` verb in `00006`'s grant is unused.
--   * `is_first_party` has no publisher flow, and `00002_publishers.sql:18-23`
--     names the reason: "a first-party service is a fact about the cafaye
--     fleet's own manifests, decided by a reviewed commit to
--     `registry/index.yml`, and never by anybody who registers an account."
--     No registration webhook exists — Phase 1 is official-only by decision.
--   * `verified` and `claimed_at` are pinned for the reason D21 and D22 carry,
--     and it is `00002`'s own sentence rather than a new opinion: "a policy
--     that branches on a column the publisher can set is a policy the publisher
--     controls." `publishers_public_read` branches on `verified`, so a publisher
--     that could write `verified` chose its own public visibility — measured
--     above, `verified = false` succeeded, and so did the inverse.
--
-- IF A FLOW EVER NEEDS ONE OF THESE, IT MUST WIDEN THIS CLAUSE ON PURPOSE. That
-- is the tripwire, and it is the reason the check is `false` rather than a
-- list: a list invites an edit that adds one column and reads as a decision,
-- while `false` refuses everything until someone writes down which flow.
--
-- WHAT THIS DOES NOT TOUCH, and the two shapes are the reason the fix is safe to
-- apply. `pantry_admin` reaches these writes through `publishers_admin_update`,
-- which this migration does not recreate: the rename that follows an upstream
-- GitHub rename, the `verified` write that an OAuth callback makes, and the
-- `is_first_party` write a reviewed commit makes all still work, and
-- `tests/rls.sh` E13..E15 execute all three rather than describing them.
--
-- +goose Up

-- Dropped and recreated rather than `alter policy`, because `alter policy` can
-- only replace a whole expression and the point of this file is that the reader
-- sees the entire `with check` at once. `drop … if exists` keeps the Up
-- re-runnable over a database where a previous run was interrupted.
drop policy if exists publishers_publisher_update on pantry.publishers;

create policy publishers_publisher_update
  on pantry.publishers
  for update
  to pantry_publisher, pantry
  -- THE `using` CLAUSE STAYS, and it is the half of the rule that is still
  -- doing work: it is what makes this "the publisher may write to its OWN row"
  -- rather than "the publisher may write to every row and be refused", and it is
  -- the half that scopes `pantry`'s maintenance reads to one publisher. `false`
  -- alone would be an equally strong statement and a less informative one.
  using (id = (select pantry.current_publisher_id()))
  -- THE BARRIER. `false` refuses every resulting row, so every UPDATE by
  -- `pantry_publisher` and by the owner `pantry` fails with SQLSTATE 42501
  -- "new row violates row-level security policy for table \"publishers\"" — a
  -- POLICY refusal, attributable to this clause, rather than a unique-index
  -- collision that attributes it to arithmetic about somebody else's row.
  --
  -- Both roles are named, and that is inherited from `00006` rather than
  -- chosen here. It is what makes the owner's maintenance obey the same rule as
  -- everybody else's instead of being the one identity in the schema that
  -- bypasses the boundary it wrote (D31), and `tests/rls.sh` F10 executes it as
  -- the table owner, because the D tier structurally cannot see that exemption.
  with check (false);

-- +goose Down

-- `00006`'s policy, byte for byte. This Down returns the database to the state
-- before this migration, which is a WEAKER boundary, and that is the correct
-- inverse: a Down that kept the stricter check would mean rolling back a
-- security tightening silently granted the hole back.
--
-- It also means the rollback is loud rather than silent, which is the property
-- worth having. `tests/rls.sh` D18..D23 and F10 run against this table, so a
-- database that has had this migration rolled back reports ten failures naming
-- themselves, instead of accepting the write and looking healthy.
drop policy if exists publishers_publisher_update on pantry.publishers;

create policy publishers_publisher_update
  on pantry.publishers
  for update
  to pantry_publisher, pantry
  using (id = (select pantry.current_publisher_id()))
  with check (id = (select pantry.current_publisher_id()));