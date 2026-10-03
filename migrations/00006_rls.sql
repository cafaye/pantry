-- 00006_rls.sql — the boundary. Trust enforced by policy, not by topology.
--
-- THE THREE ROLES, AND WHAT EACH ONE IS *PHYSICALLY* ABLE TO DO. MVP-SCOPE: "The
-- public read role physically cannot write; the publisher role is scoped to its
-- own rows; the admin role is separate. One service, one binary, one database."
-- Each half of that sentence is a different kind of guarantee and they are worth
-- separating before reading the policies:
--
--   public    CANNOT WRITE because there is no grant. Not "a policy denies it" —
--             no INSERT/UPDATE/DELETE privilege exists on any of these tables for
--             `pantry_public`, and RLS is not even reached: Postgres rejects the
--             statement at permission-check time. A bug in a policy, a new
--             SECURITY DEFINER function, or a mistaken `grant all` in a later
--             migration all fail closed here rather than open. The RLS policy on
--             top exists so that `pg_policies` answers "what may public read?" and
--             so that a `pantry_public` role that somehow acquires a write grant
--             STILL writes nothing.
--   publisher scoped by POLICY. It is granted DML and every policy's predicate is
--             `publisher_id = (select pantry.current_publisher_id())`. This one is
--             the real dependency on the policies being correct, and it is the
--             reason `00007` runs the denials as the role rather than asserting
--             the policy text.
--   admin     unconditional. It exists so "who reviews a submission" has an
--             answer that is not `pantry` itself — `pantry` owns every table, so
--             without a separate admin role the review path and the deploy path
--             would be the same identity and a migration could publish whatever it
--             liked with nobody deciding anything.
--
-- FORCE ROW LEVEL SECURITY. DECIDED HERE, AND THE DECISION IS **YES, FORCE, ON
-- EVERY TABLE** — the reasoning is in DECISIONS.md as D31 and the short version
-- is this: every service in this fleet runs its migrations as the role that owns
-- its tables, so without FORCE the owner walks straight past its own policies and
-- the table still reads as protected in `pg_class`. It is `relforcerowsecurity` —
-- the only catalog that says otherwise — and no lint in this fleet checks it.
--
-- THE COST, WHICH IS REAL AND WHICH `00003`'s first-party rule MAKES WORSE, is
-- that the owner reads ZERO rows from a protected table once FORCE is on, because
-- no policy names it and the exemption is gone. `pantry` does backfills, and it
-- does them as its own role. So the answer is NOT "don't FORCE" — an unfORCEd
-- table is a table whose policy is a comment — and it is not "the owner bypasses
-- RLS" either. It is: **pantry's own maintenance path sets the identity the same
-- way a request does**, via `begin_publisher/1`, or goes through a small number of
-- SECURITY DEFINER functions that are the documented way to do a backfill. The
-- consequence is written into the policies below: `pantry` is named in the
-- publisher policies, so `pantry` reads exactly one publisher's rows at a time
-- rather than every row — which is the same rule every other role obeys and the
-- only version of "the owner may read" that is not "the owner may read
-- everything".
--
-- ONE POLICY PER COMMAND, NEVER `for all`. kit's substrate records the measured
-- reason and it is the reason this file is written this way: the `for all` shape
-- with `using (true)` and a `with check` is what silently permits an INSERT nobody
-- meant to permit, and it is the shape every "permissive RLS policy" lint is
-- written to flag. A policy added for one command cannot widen another.
--
-- `(select …)` ON EVERY HELPER CALL. Not style. A bare function call in a policy
-- qualifier is evaluated once per candidate row; the wrapped form is hoisted into
-- an InitPlan and evaluated once per statement. Supabase's measured table puts it
-- at 178,000 calls / 12ms wrapped against 179 / 9ms for the cheapest unwrapped
-- case, and a join direction nobody measured properly has been seen at 450x. The
-- fix is structural and it is free now and expensive after the data is real.

-- =============================================================================
-- TABLE 1/4 — publishers
-- =============================================================================

-- FORCE FIRST, and `00007` asserts it by RUNNING a denial as the owner. That is
-- the only assertion that cannot be satisfied by a policy that exists.
-- +goose Up

alter table pantry.publishers enable row level security;
alter table pantry.publishers force  row level security;

-- PUBLIC READ: only third-party publishers, and only verified ones.
--
-- WHY THE STATE FILTER IS HERE AND NOT IN THE QUERY. A policy qualifier is a
-- filter on every candidate row, so putting "only verified third-party
-- publishers" in the policy rather than in the caller's `where` means the index
-- can be used and means a caller that forgets the filter still cannot publish an
-- unverified account's identity to the internet. It is the addendum's argument
-- applied to the read path: the constraint belongs to the database.
create policy publishers_public_read
  on pantry.publishers
  for select
  to pantry_public
  using (not is_first_party and verified);

-- A first-party publisher is a row the fleet creates, and there is a `where
-- is_first_party` question: should the public see that `pantry_admin`'s github
-- account exists? It is the fleet's own accounts, the list is not a secret, and
-- hiding them would make `/v1/services` render a service with no visible owner.
-- So the public policy above is restricted to third-party and this one admits the
-- first-party rows. It is a separate policy rather than a wider one because two
-- policies combine PERMISSIVELY — a wider single policy would make the shape of
-- this decision invisible in `pg_policies`.
create policy publishers_public_read_first_party
  on pantry.publishers
  for select
  to pantry_public
  using (is_first_party);

-- THE PUBLISHER: its own row and no other. Both directions, and `with check` on
-- the write is what stops a publisher from moving a row OUT of its own account by
-- updating `github_id`/`github_login` — `using` alone would permit the update and
-- `with check` is what refuses the new value.
--
-- `pantry` is in the role list because FORCE removed its exemption, so naming it
-- is what keeps the owner's reads SCOPED rather than absent (00005's header).
create policy publishers_publisher_select
  on pantry.publishers
  for select
  to pantry_publisher, pantry
  using (id = (select pantry.current_publisher_id()));

create policy publishers_publisher_update
  on pantry.publishers
  for update
  to pantry_publisher, pantry
  using (id = (select pantry.current_publisher_id()))
  with check (id = (select pantry.current_publisher_id()));

-- THE ADMIN: unconditional, one policy per command, and named rather than left to
-- PUBLIC — an unnamed policy is evaluated for every role including the ones that
-- should never reach the table.
create policy publishers_admin_select
  on pantry.publishers for select to pantry_admin using (true);
create policy publishers_admin_insert
  on pantry.publishers for insert to pantry_admin with check (true);
create policy publishers_admin_update
  on pantry.publishers for update to pantry_admin
  using (true) with check (true);
create policy publishers_admin_delete
  on pantry.publishers for delete to pantry_admin using (true);

-- DELETE IS REFUSED ENTIRELY, and this is not an omission. A publisher who
-- deletes their account row takes every service they own with it: `services
-- .publisher_id` is `on delete restrict`, so the delete fails at the FK — and a
-- delete that fails is the correct answer, because "I want my services gone" is
-- not the same request as "I want to stop being able to publish", and the second
-- is `update … set state = 'unlisted'`. The absence of a publisher DELETE policy is
-- load-bearing and `00007` asserts it.

-- =============================================================================
-- TABLE 2/4 — services
-- =============================================================================
alter table pantry.services enable row level security;
alter table pantry.services force  row level security;

-- PUBLIC READ, via the one predicate every read path shares. This is the addendum's
-- "one schema, one read path": the query does not ask "is this first-party?" and
-- the policy does not ask "is the caller trusted?" — the same predicate answers
-- both, and `GET /v1/services` and the ingest's own audit query run the identical
-- expression.
--
-- The `(select …)` wrap is on the FUNCTION call, which is the expensive part. The
-- row itself is a composite passed to a stable sql function, which the planner
-- inlines, so there is no per-row plpgsql call here at all.
create policy services_public_read
  on pantry.services
  for select
  to pantry_public
  using (pantry.service_is_visible(services));

create policy services_publisher_select
  on pantry.services
  for select
  to pantry_publisher, pantry
  using (publisher_id is not null
         and publisher_id = (select pantry.current_publisher_id()));

-- A publisher may INSERT its own row and only its own row. Two things are being
-- asserted in one `with check` and both are necessary:
--
--   publisher_id = current_publisher_id()   the row is mine
--   trust = 'third_party'                    …and I did not make it first-party
--
-- THE SECOND CLAUSE IS THE TRUST FLAG'S WHOLE ENFORCEMENT. `is_first_party` on
-- `publishers` and `trust` on `services` are the column and the row's copy of it,
-- and without this clause a publisher could insert a service with
-- `trust = 'first_party'` and be rendered in the catalog beside the fleet's own
-- services — the one impersonation this table has. Note the ordering: the CHECK
-- constraint in `00003` refuses a first-party row WITH a publisher, so the two
-- mechanisms agree rather than one shadowing the other.
--
-- `state` is deliberately NOT constrained. A publisher creating a row as
-- `published` is a real workflow question and it is not this migration's to
-- settle: MVP-SCOPE's pipeline is draft → submitted → published, and the last hop
-- is the review that `pantry_admin` performs. Constraining the insert to `draft` is
-- the obvious version and it is wrong — it would make the reviewer's approve action
-- an UPDATE that a publisher policy could also write. Recorded in DECISIONS.md as
-- the open half of D31.
create policy services_publisher_insert
  on pantry.services
  for insert
  to pantry_publisher, pantry
  with check (publisher_id = (select pantry.current_publisher_id())
              and trust = 'third_party');

-- UPDATE. `using` scopes the row being changed; `with check` scopes the row after.
-- The publisher may change its own row's metadata and state — publishing a
-- submission IS an update — but it may NOT move the row out of its own publisher
-- and it may NOT promote itself to first-party. Both are in the `with check`.
create policy services_publisher_update
  on pantry.services
  for update
  to pantry_publisher, pantry
  using (publisher_id = (select pantry.current_publisher_id()))
  with check (publisher_id = (select pantry.current_publisher_id())
              and trust = 'third_party');

-- DELETE IS REFUSED TO PUBLISHERS, and here the reason is different from
-- `publishers`. `services.service_versions.publisher_id`… does not exist; the
-- versions cascade, and a delete here destroys the version history that the yank
-- semantics are built on. MVP-SCOPE: "a yanked release must stay visible — you
-- cannot take it back from someone already running it", and deleting the service
-- deletes the versions with it. So a publisher's withdrawal is `state =
-- 'unlisted'` and a real delete is `pantry_admin`'s, and `00007` asserts both
-- halves: the publisher's delete is refused AND the rows are still there after it.
create policy services_publisher_select_no_delete_marker
  on pantry.services
  for delete
  to pantry_publisher, pantry
  using (false);

create policy services_admin_select
  on pantry.services for select to pantry_admin using (true);
create policy services_admin_insert
  on pantry.services for insert to pantry_admin with check (true);
create policy services_admin_update
  on pantry.services for update to pantry_admin
  using (true) with check (true);
create policy services_admin_delete
  on pantry.services for delete to pantry_admin using (true);

-- =============================================================================
-- TABLE 3/4 — service_versions
-- =============================================================================
alter table pantry.service_versions enable row level security;
alter table pantry.service_versions force  row level security;

-- The version row has no `publisher_id` — the trust is its service's. So the
-- policy is a SUBQUERY, and that is the one place in this directory where a policy
-- needs a join, and it is the shape Supabase's performance doc measures hardest.
--
-- THE SUBQUERY IS WRAPPED, and that is the whole reason this is not the 450x case:
--
--   service_id in (select id from pantry.services where <predicate>)   -- per row
--   service_id in (select id …)                                          -- hoisted
--
-- More precisely: the inner `(select pantry.service_is_visible(services))` is the
-- expensive predicate and it is wrapped, so the planner evaluates the visibility
-- question ONCE and produces a set, then the outer `in` is a hashed semi-join.
-- Written the other way round — filtering `services` per candidate version row —
-- the same query re-evaluates visibility for every version of every service.
create policy service_versions_public_read
  on pantry.service_versions
  for select
  to pantry_public
  using (service_id in (
    select id from pantry.services s where pantry.service_is_visible(s)
  ));

-- A YANKED VERSION IS STILL VISIBLE, and this policy does not filter on
-- `yanked_at`. That is the MVP-SCOPE requirement stated as a policy: the row stays
-- readable so somebody running it learns WHY, and the `yank_reason` is the payload
-- for that message. A read that hid yanked rows would make the yank invisible to
-- exactly the people it exists to inform. The API decides whether to show a
-- yanked version in a LISTING; the policy decides who may read it, and everybody
-- may.
create policy service_versions_publisher_select
  on pantry.service_versions
  for select
  to pantry_publisher, pantry
  using (service_id in (
    select id from pantry.services s
     where s.publisher_id = (select pantry.current_publisher_id())
  ));

-- INSERT / UPDATE / DELETE for a publisher, all scoped the same way.
--
-- NO `is_first_party` CLAUSE HERE, and the absence is deliberate in the opposite
-- direction from `services`: adding a version row to a FIRST-PARTY service is what
-- the ingest does, and the ingest runs as `pantry`. The `using` clause above
-- requires the service to belong to the caller's publisher, so a publisher cannot
-- add a version to a first-party service — which is correct, because a first-party
-- service has no publisher.
create policy service_versions_publisher_insert
  on pantry.service_versions
  for insert
  to pantry_publisher, pantry
  with check (service_id in (
    select id from pantry.services s
     where s.publisher_id = (select pantry.current_publisher_id())
  ));

create policy service_versions_publisher_update
  on pantry.service_versions
  for update
  to pantry_publisher, pantry
  using (service_id in (
    select id from pantry.services s
     where s.publisher_id = (select pantry.current_publisher_id())
  ))
  with check (service_id in (
    select id from pantry.services s
     where s.publisher_id = (select pantry.current_publisher_id())
  ));

-- DELETE IS REFUSED, for the same reason it is refused on `services`: versions are
-- yanked, not deleted. A DELETE here would take a release away from somebody
-- running it, which is the one thing MVP-SCOPE forbids outright.
create policy service_versions_publisher_no_delete
  on pantry.service_versions
  for delete
  to pantry_publisher, pantry
  using (false);

create policy service_versions_admin_select
  on pantry.service_versions for select to pantry_admin using (true);
create policy service_versions_admin_insert
  on pantry.service_versions for insert to pantry_admin with check (true);
create policy service_versions_admin_update
  on pantry.service_versions for update to pantry_admin
  using (true) with check (true);
create policy service_versions_admin_delete
  on pantry.service_versions for delete to pantry_admin using (true);

-- =============================================================================
-- TABLE 4/4 — service_compat. THE MOAT, AND THE ONLY TABLE WITH NO PUBLIC READ
-- POLICY AT ALL.
-- =============================================================================

-- NO PUBLIC READ POLICY, and this is a decision worth defending because it looks
-- like an omission and it is not.
--
-- The compatibility graph's value is "which services can I run that compose with
-- what I already run". That answer is only true if it is an answer about
-- PUBLISHED services. An edge into a draft is a fact about somebody's unreleased
-- work; publishing it turns "we are considering building this" into a public
-- roadmap and turns a published row into an attacker-selected target — the graph
-- is a map of what every service depends on, and a partial map of it is a map of
-- the parts somebody has not finished hardening.
--
-- So the graph is readable through its services' own visibility: an edge is
-- visible exactly when its `target_id` service is visible. The FIRST hop is
-- filtered by `service_id` — the service you already know about, which you can
-- see because you could see it in the catalog.
--
-- A publisher reads the edges of its own services. An admin reads everything.
alter table pantry.service_compat enable row level security;
alter table pantry.service_compat force  row level security;

create policy service_compat_publisher_select
  on pantry.service_compat
  for select
  to pantry_publisher, pantry
  using (service_id in (
    select id from pantry.services s
     where s.publisher_id = (select pantry.current_publisher_id())
  ));

create policy service_compat_publisher_insert
  on pantry.service_compat
  for insert
  to pantry_publisher, pantry
  with check (service_id in (
    select id from pantry.services s
     where s.publisher_id = (select pantry.current_publisher_id())
  ));

-- UPDATE and DELETE on the graph: DELETE is permitted for a publisher on its own
-- service's edges, and this is the ONE delete in this directory that a publisher
-- gets, because an edge is a publisher's own claim about its own service and
-- withdrawing a claim is not taking anything away from a user — nothing user-facing
-- depends on an edge having been there, unlike a version. `required_by` is the
-- query that would show a hole, which is why `00007` asserts that withdrawing an
-- edge from A does not hide A from B's `required_by`.
create policy service_compat_publisher_update
  on pantry.service_compat
  for update
  to pantry_publisher, pantry
  using (service_id in (
    select id from pantry.services s
     where s.publisher_id = (select pantry.current_publisher_id())
  ))
  with check (service_id in (
    select id from pantry.services s
     where s.publisher_id = (select pantry.current_publisher_id())
  ));

create policy service_compat_publisher_delete
  on pantry.service_compat
  for delete
  to pantry_publisher, pantry
  using (service_id in (
    select id from pantry.services s
     where s.publisher_id = (select pantry.current_publisher_id())
  ));

create policy service_compat_admin_select
  on pantry.service_compat for select to pantry_admin using (true);
create policy service_compat_admin_insert
  on pantry.service_compat for insert to pantry_admin with check (true);
create policy service_compat_admin_update
  on pantry.service_compat for update to pantry_admin
  using (true) with check (true);
create policy service_compat_admin_delete
  on pantry.service_compat for delete to pantry_admin using (true);

-- =============================================================================
-- THE GRANTS. Read this before concluding the policies are enough.
-- =============================================================================

-- =============================================================================
-- SCHEMA USAGE, AND WHY IT IS ONE LIST GRANTED ONCE AND NEVER REVOKED
-- =============================================================================
--
-- USAGE IS NOT A TABLE PRIVILEGE AND OWNING THE OBJECTS DOES NOT SUPPLY IT. This
-- file used to say the opposite, in these words:
--
--     `pantry` ITSELF GETS NOTHING BY THIS MIGRATION. It owns the tables, so it
--     can already do all of it … A `grant … to pantry` here would be a statement
--     that the owner needs privileges it does not have, which is false and would
--     mislead the next reader.
--
-- The argument is CORRECT ABOUT TABLE PRIVILEGES — an owner does hold SELECT,
-- INSERT, UPDATE and DELETE on its tables implicitly, and that is why there is
-- still no `grant … to pantry` on the tables below. It is WRONG ABOUT THE SCHEMA,
-- and it was wrong in the most expensive direction, because it did not merely omit
-- a statement: it argued that the omission was correct.
--
-- HERE IS WHY THE ARGUMENT LOOKED RIGHT, which is the part worth keeping. The
-- migrations create the schema and the tables, so in a development cluster the
-- role that runs them owns both, and an owner of a schema holds every privilege on
-- it implicitly — `has_schema_privilege('pantry', 'pantry', 'USAGE')` answers true
-- without a grant, because the owner's implicit privileges are not in `nspacl`.
-- On that cluster the line above is visibly true and the grant would look
-- redundant. In production the schema is NOT owned by `pantry`, and there the
-- grant is the only thing that supplies USAGE:
--
--   * `00001` needs `create role`, so the migrations cannot be applied by
--     `pantry` itself. Measured: applying this directory connected as `pantry`
--     fails with `permission denied to create role` at `00001`'s `do $$ … $$`.
--     So migrations run as a provisioning role.
--   * Objects a provisioning role creates are owned by that provisioning role, and
--     this directory contains no `alter … owner to`. Ownership is therefore
--     whatever the operator did, and a role that is not the schema owner gets its
--     USAGE from `PUBLIC` — which the next line revokes.
--
-- So the two lines combined were: take USAGE away from every role, including the
-- owner, and grant it back to three of the four roles that exist. Measured on
-- PostgreSQL 18.4, schema `pantry` owned by the provisioning role and the tables
-- handed to `pantry`:
--
--     set role pantry_public; select count(*) from pantry.services;
--       ERROR:  permission denied for schema pantry
--     set role pantry;      select count(*) from pantry.services;
--       ERROR:  permission denied for schema pantry
--
-- Both roles locked out of a schema whose tables both policies and the owner were
-- written for. A registry with no data path: every read is a 403 and every write
-- is a 403, and nothing in `pg_policies` says so.
--
-- ONE `revoke`, THEN ONE `grant` TO ALL FOUR ROLES, AND NOTHING AFTERWARDS. The
-- previous file also granted `pantry_public` its USAGE at line 425 and then
-- `revoke`d it back off at line 441, under a comment about DDL that the revoke did
-- not achieve — `revoke all on schema pantry from pantry_public` removes `USAGE`,
-- which is exactly the privilege that made the SELECT grant above it reachable, and
-- leaves `pantry_public` holding nothing. That is the same lockout, one role
-- narrower, and it was in the file this packet was asked to fix. A revoke written
-- to subtract one privilege and subtracting another is why the four roles are named
-- together here: a reader can see the whole set in one statement and cannot miss
-- one.
revoke all on schema pantry from public;

-- USAGE AND NOT CREATE, for all four roles. `create` on a schema is the privilege
-- this file refuses `pantry_public` and it refuses it for every role here rather
-- than only for the public one: a role that can create objects in this schema can
-- create an object that shadows a table name for the next session in some
-- `search_path` orderings, and no policy protects against that because the shadow
-- is never a policy-controlled row. Not `usage on schema public` either — a read
-- role has no business in the `public` schema of a cluster it does not own.
grant usage on schema pantry to pantry_public, pantry_publisher, pantry_admin, pantry;

-- `tests/rls.sh` asserts the shape of this section rather than the text of it:
-- every role named by any policy in this schema holds USAGE on it, and every
-- command any policy allows has a matching table privilege for the role that
-- policy names. Both assertions are derived from `pg_policies`, so they keep
-- holding as tables and policies are added. Both are red on the version of this
-- file above, and `00001`'s Down note in `tests/rls.sh` records that.

-- =============================================================================
-- THE TABLE GRANTS, ONE ROLE AT A TIME
-- =============================================================================

-- `pantry_public` GETS SELECT AND NOTHING ELSE. On every table.
--
-- This is the sentence "the public read role physically cannot write", and it is
-- enforced HERE rather than by a policy: a role with no INSERT privilege cannot
-- INSERT, and RLS is never consulted. The policies above add a second, independent
-- barrier, so the two have to both fail for a write to succeed.
--
-- NO `grant select on sequence` either. The ids are uuid with
-- `gen_random_uuid()`, so there is no sequence to read and no sequence to grant;
-- that is a design choice made in `00003` whose security value is that the grant
-- list here is four verbs and cannot grow by accident.
grant select on
  pantry.services,
  pantry.service_versions,
  pantry.publishers,
  pantry.service_compat
  to pantry_public;

-- `pantry_publisher` GETS DML, AND THE GRANT IS DELIBERATELY NOT `all`.
grant select, insert, update on
  pantry.services,
  pantry.service_versions,
  pantry.service_compat
  to pantry_publisher;
grant select, update on
  pantry.publishers
  to pantry_publisher;
-- NO DELETE IN THAT LIST, and it is not an oversight: `services` and
-- `service_versions` have a `using (false)` delete policy, and a role with no
-- DELETE privilege cannot reach the policy at all. `service_compat` DOES get
-- DELETE, matching its policy above. Three tables, three different answers, and
-- the grant list is where a reader can see that rather than having to reconstruct
-- it from four policies.

-- `pantry_admin` GETS DML ON ALL FOUR TABLES, AND THIS GRANT WAS MISSING.
--
-- Eight `pantry_admin` policies exist above and are correct; the role held NO
-- table privilege at all, so Postgres rejected every statement at permission-check
-- time and never consulted a policy. `pantry_admin` could not review a submission,
-- could not publish one, and could not run the first-party ingest — the three
-- things it exists for. `\dp` reads as a complete admin boundary, which is the
-- point: the boundary looked finished because the policies were finished, and the
-- grant list was the only place the gap was visible and nothing was reading it.
--
-- The grant MIRRORS THE POLICIES rather than widening past them: every command the
-- four admin policy sets allow, on every table they cover, and nothing else. The
-- narrow-grant instinct above is kept, not abandoned — `pantry_admin` gets no
-- CREATE on the schema, and it gets no sequence and no other schema.
grant select, insert, update, delete on
  pantry.services,
  pantry.service_versions,
  pantry.publishers,
  pantry.service_compat
  to pantry_admin;

-- =============================================================================
-- `pantry` ITSELF: THE OWNER, AND THE ROLE/OPERATION MAP THIS FILE ASSUMES
-- =============================================================================
--
-- `pantry` GETS `USAGE` ABOVE AND NO TABLE GRANT HERE, and the second half is not
-- an omission: an owner holds SELECT/INSERT/UPDATE/DELETE on its own tables
-- implicitly, so a table grant to `pantry` would be a statement about a privilege
-- it already has. It would also be a second, weaker copy of the first-party claim,
-- because a grant is visible in `\dp` and an owner's implicit privileges are not.
--
-- WHAT `pantry` CAN DO, and it is a short list, written down because the whole
-- point of `FORCE` is that the answer is not "everything it owns":
--
--   DDL — yes, in full. It owns the objects, RLS does not apply to DDL, and this
--         is the migration path: `00001`'s Down, the enum migrations' `do $$ … $$`
--         guards and every future migration run as this role. It is also the only
--         role that can read `pg_policies` reasons into effect here.
--   Reads and writes scoped to ONE publisher, once `begin_publisher/1` has been
--         called — yes, because the publisher policies name `pantry` alongside
--         `pantry_publisher`. That naming is not a convenience: it is what makes a
--         backfill run as the service's own role obey exactly the rule every other
--         role obeys, instead of the owner being the one role in this schema that
--         bypasses the boundary it wrote.
--   Reads with NO identity set — ZERO rows. `current_publisher_id()` returns NULL
--         and every publisher predicate compares against NULL. This is the accepted
--         cost of `FORCE` (DECISIONS.md D31) and `tests/rls.sh` asserts it rather
--         than leaving it to be discovered.
--   INSERT — NO POLICY OF ITS OWN, DELIBERATELY. This is the decision this packet
--         had to make rather than mechanically make, so here it is in full.
--
-- WHY `pantry` HAS NO INSERT POLICY. Ingesting the fleet's own manifests is a
-- FIRST-PARTY operation, and a first-party service is a row with
-- `publisher_id IS NULL` — `00003`'s `services_first_party_has_no_publisher`
-- CHECK forbids the alternative. Every publisher-shaped insert policy in this file
-- requires `publisher_id = current_publisher_id()`, so by construction none of them
-- can admit a first-party row, and a policy written to let `pantry` insert one
-- would be an unconditional one. The two candidates were:
--
--   (a) an unconditional INSERT policy for `pantry`. Rejected: it makes the role
--       that OWNS the tables and RUNS the migrations able to create rows it has no
--       business creating, on every table, forever — which is the bypass the
--       `FORCE` decision exists to prevent, reached through a policy instead of
--       through `relforcerowsecurity = false`.
--   (b) no INSERT policy for `pantry`, and the ingest runs as `pantry_admin`.
--       Chosen. `pantry_admin` already holds `services_admin_insert with check
--       (true)` and now holds the grant for it, so a first-party publisher, a
--       first-party service, its versions and its compatibility edges are all
--       creatable — by the role whose entire job is the decision that a row
--       describing a fleet service exists. `tests/rls.sh` asserts the ingest runs
--       green as `pantry_admin` AND is refused as `pantry`, because the second half
--       is the only thing that makes the first half a decision rather than an
--       accident of who happened to hold a grant.
--
-- The cost of this choice, stated rather than discovered later: the fleet's
-- manifests arrive in the database through a role that is not the one serving
-- HTTP, so the sync needs a login that is a member of `pantry_admin`. That is one
-- provisioning statement and `00001` already says credentials are the cluster's
-- business. `pantry` is `NOINHERIT`, so a membership in `pantry_admin` does not
-- leak into the serving role even if the same login is granted both.
--
-- Publisher SELF-REGISTRATION is the other thing no role may do, and the absence
-- is MVP-SCOPE's rather than this migration's: Phase 1 is official-only, "adding a
-- service is a commit and a reviewed pull request", so `publishers` rows are
-- created by a migration, a fixture or an admin. `publishers` therefore has no
-- publisher INSERT policy at all, and `tests/rls.sh` asserts both that `pantry` and
-- `pantry_publisher` are refused one and that `pantry_admin` is not.

-- +goose Down

-- DISABLE BEFORE DROP POLICY, in that order and not the other way round, for a
-- reason kit's substrate Down records and this file inherits: a table with
-- policies and no RLS enabled hides every row from everybody, so dropping first
-- would leave four tables unreadable in the window between the two statements.
--
-- The grants are NOT revoked, for the same reason identity's Down does not revoke
-- its grants: a Down that took them away would leave `pantry_public` holding a
-- grant it must not be able to use, which is a weaker state than either side of
-- this migration. `pantry_public`'s usefulness comes from being able to SELECT and
-- from holding nothing else, and it holds nothing else whether or not this Down
-- runs.
alter table pantry.service_compat  disable row level security;
alter table pantry.service_versions disable row level security;
alter table pantry.services        disable row level security;
alter table pantry.publishers      disable row level security;

drop policy if exists service_compat_admin_delete        on pantry.service_compat;
drop policy if exists service_compat_admin_update         on pantry.service_compat;
drop policy if exists service_compat_admin_insert         on pantry.service_compat;
drop policy if exists service_compat_admin_select         on pantry.service_compat;
drop policy if exists service_compat_publisher_delete     on pantry.service_compat;
drop policy if exists service_compat_publisher_update     on pantry.service_compat;
drop policy if exists service_compat_publisher_insert     on pantry.service_compat;
drop policy if exists service_compat_publisher_select     on pantry.service_compat;

drop policy if exists service_versions_admin_delete      on pantry.service_versions;
drop policy if exists service_versions_admin_update       on pantry.service_versions;
drop policy if exists service_versions_admin_insert       on pantry.service_versions;
drop policy if exists service_versions_admin_select       on pantry.service_versions;
drop policy if exists service_versions_publisher_no_delete on pantry.service_versions;
drop policy if exists service_versions_publisher_update   on pantry.service_versions;
drop policy if exists service_versions_publisher_insert   on pantry.service_versions;
drop policy if exists service_versions_publisher_select   on pantry.service_versions;

drop policy if exists services_admin_delete               on pantry.services;
drop policy if exists services_admin_update                on pantry.services;
drop policy if exists services_admin_insert                on pantry.services;
drop policy if exists services_admin_select                on pantry.services;
drop policy if exists services_publisher_select_no_delete_marker on pantry.services;
drop policy if exists services_publisher_update            on pantry.services;
drop policy if exists services_publisher_insert            on pantry.services;
drop policy if exists services_publisher_select            on pantry.services;
drop policy if exists services_public_read                 on pantry.services;

drop policy if exists publishers_admin_delete             on pantry.publishers;
drop policy if exists publishers_admin_update              on pantry.publishers;
drop policy if exists publishers_admin_insert              on pantry.publishers;
drop policy if exists publishers_admin_select              on pantry.publishers;
drop policy if exists publishers_publisher_update          on pantry.publishers;
drop policy if exists publishers_publisher_select          on pantry.publishers;
drop policy if exists publishers_public_read_first_party   on pantry.publishers;
drop policy if exists publishers_public_read               on pantry.publishers;