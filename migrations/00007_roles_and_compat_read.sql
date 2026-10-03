-- 00007_roles_and_compat_read.sql — the two things the directory owed, and both
-- of them are omissions rather than decisions.
--
-- This migration is late in the sequence on purpose. It grants a MEMBERSHIP
-- rather than a role, and it adds a SELECT policy rather than a table, so
-- nothing before it references anything here. A reader who stops at 00006 finds
-- a complete-looking schema; that completeness was the bug.
--
-- =============================================================================
-- PART 1 — `grant pantry_public to pantry`
-- =============================================================================
--
-- WHY A MIGRATION AND NOT A PROVISIONING STEP, which is the question this part
-- exists to answer. It could have gone either way, and the argument for
-- putting it here is that the alternative was already tried and does not work:
--
--   `tests/rls.sh --serve` performed this grant by hand, loudly, with a comment
--   naming it as "THE ONE PROVISIONING STATEMENT THE MIGRATIONS DO NOT
--   CONTAIN". That is a confession in the test harness. A grant the test suite
--   has to perform before the code under test can start is not a deployment
--   detail — it is a step that is missing from the thing that is supposed to
--   define the database, and the only reason it went unnoticed is that the
--   harness supplied it. Somebody building a database from this directory with
--   `goose up` and nothing else got `permission denied to set role`.
--
-- MEASURED, on PostgreSQL 18.4, applying only the `-- +goose Up` sections of
-- 00001-00006 to an empty database:
--
--   memberships in pg_auth_members mentioning a pantry role: 0 rows
--   set session authorization pantry; set role pantry_public;
--     ERROR:  permission denied to set role "pantry_public"
--   has_table_privilege('pantry_public','pantry.services','SELECT'): t
--
-- The last line is why this is a blocker and not an inconvenience. The grant
-- exists; the MEMBERSHIP does not. The database can answer every question about
-- what `pantry_public` is allowed to do, and the answer is "SELECT", and no
-- connection can ever become it.
--
-- WHY IT IS NOT IN 00001, which is where a reader will first look. `00001`
-- creates the roles and asserts NOINHERIT on all four, and the temptation is to
-- put a grant next to the `create role` it relates to. Two reasons not to:
--
--   1. `00001`'s own Down does not drop the roles (deliberately, and correctly —
--      dropping a role takes with it every grant and membership pointing at it).
--      A grant made in 00001 therefore SURVIVES its own rollback: roll back to
--      zero and `pantry` is still a member of a role that 00001 no longer
--      describes. A Down that leaves state its own Up created is a Down that
--      does not undo. Here, the Down revokes the membership in the same breath.
--   2. Editing 00001 would not reach anybody who has already applied it. Goose
--      records versions, not checksums, so an amended 00001 is silently skipped
--      on every existing database and the bug persists exactly where it is
--      already hurting. A new file reaches all of them on the next `up`.
--
-- ONLY `pantry_public`, AND THE ABSENCE IS THE POINT. `pantry_admin` is
-- deliberately NOT granted, and `00001` says why at length: `pantry` owns the
-- tables and runs the migrations, so making the owner an admin "would make the
-- admin path and the deploy path the same identity, and a migration would be
-- able to publish whatever it liked without a decision". Granting it here would
-- undo the single most load-bearing sentence in that file.
--
-- `pantry_publisher` is not granted either, for a different reason: taking it is
-- a per-request decision made with an identity in hand (see
-- `pantry.current_publisher_id()`), and granting it to the service login would
-- make every publisher write run as a member with no publisher set — which reads
-- ZERO rows through every publisher policy, silently. The sync job that needs
-- admin gets its own login, which is an operator's to provision.
--
-- NOINHERIT IS NOT RELAXED, and this is the property that makes the grant
-- meaningful rather than redundant. `pantry` stays NOINHERIT: the membership
-- grants the ABILITY to take the role and nothing more. Without `set role`, a
-- connection authenticated as `pantry` still reads nothing, because
-- `services_public_read` names `pantry_public` and no other. That is the
-- property `00001` argues for and this grant does not weaken:
--
--   select rolname, rolinherit from pg_roles where rolname = 'pantry';  -- f
--
-- =============================================================================
-- PART 2 — the public read policy on `service_compat`
-- =============================================================================
--
-- `00006` has a section headed "NO PUBLIC READ POLICY, and this is a decision
-- worth defending because it looks like an omission and it is not". It then
-- argues, at length and correctly, for exactly the policy it declines to write:
--
--   "So the graph is readable through its services' own visibility: an edge is
--    visible exactly when its `target_id` service is visible. The FIRST hop is
--    filtered by `service_id` — the service you already know about, which you
--    can see because you could see it in the catalog."
--
-- The argument was right and the statement was missing. `00006` grants SELECT
-- on `service_compat` to `pantry_public` and writes no policy naming it, so the
-- table is simultaneously reachable and silent. Measured on the seeded
-- database, 18.4:
--
--   policies on service_compat: 8, naming {pantry_admin} or {pantry,pantry_publisher}
--   policies naming pantry_public: 0
--   has_table_privilege('pantry_public','pantry.service_compat','SELECT'): t
--   rows in service_compat: 4; rows pantry_public reads: 0
--
-- `00004_service_compat.sql` is the file that argues this registry is not a link
-- directory. The catalog role could not read the table holding the argument, so
-- the compatibility graph — the thing this registry is FOR — was invisible to
-- the only role that serves the public catalog. The query existed, generated
-- from sqlc, correct, joining across a table with nothing on the other side.
--
-- The policy is `00006`'s own sentence as a statement. It is written to require
-- BOTH endpoints visible, which is strictly stronger than "visible exactly when
-- its target is visible", and the difference is one class of row: an edge whose
-- SOURCE service is not visible. Hiding it is what the surrounding paragraph
-- argues for ("a partial map of the graph is a map of the parts somebody has not
-- finished hardening"), and a public reader has no way to name an invisible
-- source service anyway — they cannot look one up in the catalog. Being stricter
-- is safe here in the sense that matters: this policy can only remove rows
-- relative to the stated rule, never add any.
--
-- `service_is_visible` is `security invoker` and stable, so this composes: the
-- same predicate `GET /v1/services` is filtered by is the one deciding whether
-- an edge is public. There is no second definition of "visible" to drift.

-- +goose Up

-- The membership. Idempotent: `grant` on an existing membership is a notice, not
-- an error, so re-running this migration converges rather than failing.
grant pantry_public to pantry;

-- The policy. One policy per command, always — this is the command, and the only
-- one `pantry_public` gets on this table. Everything else stays denied because
-- there is no other policy and no INSERT/UPDATE/DELETE grant: a role with no
-- grant cannot write, and RLS is never even consulted. That is a stronger claim
-- than a policy that happens to deny it, and it is the same argument `00001`
-- makes about `pantry_public` holding SELECT and nothing else.
create policy service_compat_public_read
  on pantry.service_compat
  for select
  to pantry_public
  using (
    exists (select 1 from pantry.services t
             where t.id = service_compat.target_id
               and pantry.service_is_visible(t))
    and exists (select 1 from pantry.services s
                 where s.id = service_compat.service_id
                   and pantry.service_is_visible(s))
  );

-- +goose Down

-- The policy goes first. It references two tables, and dropping it before the
-- membership means the Down never leaves a policy naming a role nobody can take.
drop policy if exists service_compat_public_read on pantry.service_compat;

-- And the membership is revoked, which is the thing 00001 could not have done
-- for itself: this Down returns the database to the state before 00007 rather
-- than to a state where `pantry` still belongs to a group this directory's own
-- rollback forgot to take away. The read path stops working on purpose — a Down
-- that kept the catalog working would be a Down that did not roll back.
revoke pantry_public from pantry;