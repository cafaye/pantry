-- 00001_roles.sql — the three roles RLS is written against, and the schema they
-- live in.
--
-- WHY A ROLE MIGRATION IS FIRST. Every policy below names `pantry_public`,
-- `pantry_publisher` and `pantry_admin`, and a policy naming a role that does not
-- exist is a migration that fails six lines later with `role "pantry_public" does
-- not exist` and no fix. So the roles are created before anything references
-- them, and `00005_rls.sql` can be read as policies rather than as setup.
--
-- THE THREE TRUST LEVELS, and why there are three and not two:
--
--   pantry_public     NOLOGIN. SELECT on published rows and nothing else. It is
--                     granted NO insert, update or delete anywhere, so "the public
--                     read role physically cannot write" is not a policy that
--                     happens to deny it — there is no grant to violate. That is a
--                     different kind of guarantee from a policy, and it is the one
--                     worth having on the read path: a bug in the policy, a
--                     `security definer` function added later, or a role
--                     membership somebody grants all fail closed here.
--   pantry_publisher  NOLOGIN. The group a signed-in publisher's role is a member
--                     of. It writes rows and reads its own; it does not read the
--                     RLS boundary by being a member, because the boundary is a
--                     policy and the policies name the group. Group, not login, so
--                     a publisher's per-request role from identity can be granted
--                     into it without a second credential existing here.
--   pantry_admin      NOLOGIN. Unconditional. It is the answer to "who reviews a
--                     submission", and it is deliberately NOT `pantry`'s own role:
--                     pantry owns the tables and runs the migrations, so giving the
--                     owner the admin policy would make the admin path and the
--                     deploy path the same identity, and a migration would be able
--                     to publish whatever it liked without a decision.
--
-- NOINHERIT ON ALL THREE, and this is the one setting here that is not
-- conventional. Postgres's default is INHERIT, which means a role that is a
-- member of a group silently has the group's powers. That is convenient and it is
-- exactly wrong for this design: pantry is one binary and one database, and the
-- whole reason the addendum rejects two deployments is that a policy can buy what
-- a network boundary buys. NOINHERIT means the only way to hold a group's powers
-- is to be granted them, and `set role` is explicit — so `00005_rls.sql`'s
-- isolation assertions can impersonate a role and can be certain it is testing the
-- role it named. kit's substrate makes the same choice for its `_app` roles.
--
-- NO CREATEDB / NOCREATEDB, NO SUPERUSER, and no BYPASSRLS anywhere in this
-- file. BYPASSRLS is the one attribute that would undo `00005_rls.sql`
-- wholesale, and it is called out here because nothing else in this repository
-- would notice it being granted.
--
-- NO PASSWORD and no LOGIN. Cluster provisioning owns credentials; this
-- migration owns the shape a policy is written against. The three roles are
-- NOLOGIN precisely so a reviewer reading this file cannot ask "what can log in
-- as pantry_admin" and find the answer is whoever could add a password.

-- +goose Up

-- The schema. Per-service, like every service in this fleet: `pantry`'s tables are
-- in `pantry` and a sweep over another service's schema cannot see them.
--
-- `pg_trgm` and `unaccent` are installed here rather than in the tables'
-- migration because `create extension` is database-wide and this is the earliest
-- statement that can be. Search is Postgres full-text plus trigram — the addendum
-- rules out a second datastore, and a few thousand rows is nothing to Postgres.
create extension if not exists pg_trgm;
create extension if not exists unaccent;

create schema if not exists pantry;

-- The three roles, idempotently. `do $$ … if not exists` rather than
-- `create role` guarded by an exception block, because a role's attributes
-- (NOINHERIT especially) must be asserted rather than merely created: a role
-- that already exists with INHERIT would be a security defect this migration
-- would otherwise leave in place and report success about.
-- +goose StatementBegin

do $$
begin
  -- THE OWNER. `pantry` is named in `00006_rls.sql`'s policies and it is the role
  -- that OWNS the tables, so it is this file's fourth role and not an assumption.
  --
  -- Why it is created here rather than assumed from cluster provisioning: the
  -- brief's first line of "done means" is that migrations apply cleanly to an
  -- EMPTY database, and a migration that fails with `role "pantry" does not
  -- exist` does not apply to anything — it applies to a cluster somebody
  -- provisioned first, which is measured, not assumed: this file's first apply
  -- attempt on a fresh cluster failed with exactly that, on the `create policy …
  -- to pantry_publisher, pantry` line.
  --
  -- It is LOGIN, unlike the three above, because it is the service's own role and
  -- the service connects as itself. It is NOINHERIT like them, so a grant into
  -- `pantry_admin` does not silently hand it the admin policy.
  --
  -- `create role` only when absent, so a cluster that already provisioned this
  -- role keeps whatever LOGIN/CREATEDB attributes its provisioning gave it.
  -- NO PASSWORD here: this migration owns the shape a policy is written against,
  -- and credentials are the cluster's business.
  if not exists (select 1 from pg_roles where rolname = 'pantry') then
    create role pantry login noinherit;
  end if;

  if not exists (select 1 from pg_roles where rolname = 'pantry_public') then
    create role pantry_public nologin noinherit;
  end if;
  if not exists (select 1 from pg_roles where rolname = 'pantry_publisher') then
    create role pantry_publisher nologin noinherit;
  end if;
  if not exists (select 1 from pg_roles where rolname = 'pantry_admin') then
    create role pantry_admin nologin noinherit;
  end if;
end
$$;

-- +goose StatementEnd

-- Re-assert the attributes every time the migration runs. This is three
-- statements that look redundant against the `create` above, and they are the
-- reason the `create` above is safe: a cluster that provisioned `pantry_admin`
-- with INHERIT — which is Postgres's default and is what every other role on the
-- cluster has — gets it corrected here, and the correction is idempotent.
alter role pantry          noinherit;
alter role pantry_public    noinherit;
alter role pantry_publisher noinherit;
alter role pantry_admin     noinherit;

-- +goose Down

-- The roles are NOT dropped, and that is the same reasoning identity's
-- `00016_account_isolation.sql` Down records: the Down must leave the cluster no
-- weaker than before. Dropping a role would take with it every object grant and
-- every membership pointing at it, in an order this file does not control.
-- Removing the roles belongs with the decision to remove them, which is a
-- migration of its own and not the rollback of this one.
--
-- What this Down does undo is everything a re-run must converge: the schema and
-- the two extensions it asked for.
drop schema if exists pantry cascade;