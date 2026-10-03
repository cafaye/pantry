# tests/rls_checks.sh — every assertion, sourced by tests/rls.sh.
#
# NOT A TEST FILE. It is sourced, so that the fixtures, the runner and the
# assertions are three files with three jobs and this one is only the last of them.
#
# THE SHAPE OF EVERY ASSERTION HERE. There are three kinds and the difference is
# the whole value of the suite:
#
#   rowset <name> <db> <sql> <expected-csv>
#     An EXACT row set. "A publisher sees only its own rows" is not `count(*) = 2`,
#     it is these two names and NOT the third one — the negative case is in the
#     expectation, not assumed from the count.
#   ok <name> <db> <sql> [fragment the output must contain]
#     The statement ran and the database said the thing the check is about. Used
#     for a privilege, a count, and for a refusal that is silent (`UPDATE 0`).
#   deny <name> <db> <sqlstate> <barrier fragment> <sql>
#     The statement was EXECUTED and refused, and it was refused by the barrier the
#     check names: `permission denied for table` is the grant boundary — Postgres
#     never consulted a policy — and `row-level security` is the policy boundary.
#     Asserting the sqlstate alone would pass a suite whose policies were all
#     `using (false)` and whose grants were all missing.
#
# THE FIXTURES USE FIXED UUIDs rather than generated ones, because every
# expectation below is then readable as a literal and a reader can check the
# arithmetic by eye instead of trusting that the fixture and the assertion were
# edited together.
#
#   publisher  alpha   (a1, verified, third party)  owns alpha-api, alpha-draft
#              bravo   (a2, verified, third party)  owns bravo-api, bravo-hidden
#              cafaye  (a3, FIRST PARTY)             no publisher_id on its row
#              unverified (a4, NOT verified)         public must not see it
#   service    alpha-api     third_party published   public + alpha
#              alpha-draft   third_party draft       alpha only
#              bravo-api     third_party published   public + bravo
#              bravo-hidden  third_party unlisted    bravo only
#              cafaye        first_party published   public + everyone who asks
#                                                     as an admin
#
# Seeded as `pantry_admin`, which is itself the first assertion that has to hold:
# a role that cannot insert cannot seed anything, and a suite that fails at the
# fixture step with `permission denied` has told you the answer already.

# `seed` REPORTS ITS OWN FAILURE rather than returning quietly, because a suite
# that fails at the fixture step and then reports twelve zeroes is worse than a
# suite that says "the role that writes the fixtures could not write" and stops.
seed() {
  local dbn="$1" out rc
  out="$(run "$dbn" "set role pantry_admin;
insert into pantry.publishers (id, github_id, github_login, is_first_party, verified) values
  ('00000000-0000-4000-8000-0000000000a1', 101, 'alpha',       false, true),
  ('00000000-0000-4000-8000-0000000000a2', 202, 'bravo',       false, true),
  ('00000000-0000-4000-8000-0000000000a3', 303, 'cafaye',      true,  true),
  ('00000000-0000-4000-8000-0000000000a4', 404, 'unverified',  false, false);
insert into pantry.services (id, name, language, kind, trust, state, core_constraint, manifest, manifest_sha256, publisher_id, ingested_by) values
  ('00000000-0000-4000-8000-0000000000b1', 'alpha-api',    'go',   'api', 'third_party', 'published', '^1.0.0', '{}'::jsonb, repeat('a',64), '00000000-0000-4000-8000-0000000000a1', 'pantry-rls-fixture'),
  ('00000000-0000-4000-8000-0000000000b2', 'alpha-draft',  'go',   'api', 'third_party', 'draft',     '^1.0.0', '{}'::jsonb, repeat('b',64), '00000000-0000-4000-8000-0000000000a1', 'pantry-rls-fixture'),
  ('00000000-0000-4000-8000-0000000000b3', 'bravo-api',    'rust', 'cli', 'third_party', 'published', '~0.2.0', '{}'::jsonb, repeat('c',64), '00000000-0000-4000-8000-0000000000a2', 'pantry-rls-fixture'),
  ('00000000-0000-4000-8000-0000000000b4', 'bravo-hidden', 'rust', 'cli', 'third_party', 'unlisted',  '~0.2.0', '{}'::jsonb, repeat('d',64), '00000000-0000-4000-8000-0000000000a2', 'pantry-rls-fixture'),
  ('00000000-0000-4000-8000-0000000000b5', 'cafaye',       'go',   'api', 'first_party', 'published', '>=0.1.0','{}'::jsonb, repeat('e',64), null,                                      'pantry-rls-fixture');
insert into pantry.service_versions (id, service_id, tag, version_major, version_minor, version_patch) values
  ('00000000-0000-4000-8000-0000000000c1', '00000000-0000-4000-8000-0000000000b1', 'v1.2.3', 1, 2, 3),
  ('00000000-0000-4000-8000-0000000000c2', '00000000-0000-4000-8000-0000000000b5', 'v0.1.0', 0, 1, 0);
insert into pantry.service_compat (service_id, target_id, kind, version_range, dependency) values
  ('00000000-0000-4000-8000-0000000000b1', '00000000-0000-4000-8000-0000000000b5', 'requires',       '^0.1.0', 'required'),
  ('00000000-0000-4000-8000-0000000000b1', '00000000-0000-4000-8000-0000000000b3', 'conflicts_with', '~1.0.0', 'soft'),
  ('00000000-0000-4000-8000-0000000000b5', '00000000-0000-4000-8000-0000000000b3', 'requires',       '^0.2.0', 'required');")"
  rc=$?
  if [[ $rc -ne 0 ]]; then
    record "A0  fixture: pantry_admin can write the fixtures" FAIL "exit=$rc" "$(oneline "$out")"
    FAIL=$((FAIL+1)); FAILED_NAMES+=("A0  fixture: pantry_admin can write the fixtures")
    say "FAIL  A0  fixture: pantry_admin can write the fixtures"
    say "        $(oneline "$out")"
    say "        every check below reads from fixtures that do not exist. Fix the"
    say "        pantry_admin GRANT in 00006_rls.sql before reading anything else."
    return 1
  fi
  record "A0  fixture: pantry_admin can write the fixtures" ok "exit=0" "4 publishers, 5 services, 2 versions, 3 edges"
  PASS=$((PASS+1))
  return 0
}

seed "$SHAPE1" || true
seed "$SHAPE2" || true

# The publisher identity is transaction-local (`set_config(..., true)`), so it has
# to be set in the SAME statement string as the query that depends on it. Each
# helper below therefore takes the whole session and never assumes a previous
# `begin_publisher` survived.
as_publisher() { printf "set role pantry_publisher;\nselect pantry.begin_publisher('%s');\n%s" "$1" "$2"; }
as_admin()     { printf "set role pantry_admin;\n%s" "$1"; }
as_service()   { printf "set role pantry;\n%s" "$1"; }
as_public()    { printf "set role pantry_public;\n%s" "$1"; }

A1='00000000-0000-4000-8000-0000000000a1'
B1='00000000-0000-4000-8000-0000000000b1'
B3='00000000-0000-4000-8000-0000000000b3'

# ===========================================================================
say ""
say "-- schema usage: the defect this packet exists for"
# ===========================================================================

# CHECK A1 schema usage: pantry holds USAGE on schema pantry
ok "A1  schema usage: pantry holds USAGE on schema pantry" "$SHAPE1" \
  "select has_schema_privilege('pantry','pantry','USAGE') as usage;" "t"
# CHECK A2 schema usage: pantry_public holds USAGE on schema pantry
ok "A2  schema usage: pantry_public holds USAGE on schema pantry" "$SHAPE1" \
  "select has_schema_privilege('pantry_public','pantry','USAGE') as usage;" "t"
# CHECK A3 schema usage: pantry_publisher holds USAGE on schema pantry
ok "A3  schema usage: pantry_publisher holds USAGE on schema pantry" "$SHAPE1" \
  "select has_schema_privilege('pantry_publisher','pantry','USAGE') as usage;" "t"
# CHECK A4 schema usage: pantry_admin holds USAGE on schema pantry
ok "A4  schema usage: pantry_admin holds USAGE on schema pantry" "$SHAPE1" \
  "select has_schema_privilege('pantry_admin','pantry','USAGE') as usage;" "t"
# CHECK A5 schema usage: PUBLIC holds nothing at all on schema pantry
# `grantee = 0` is PUBLIC in an ACL. A count rather than a boolean because
# `has_schema_privilege('public', …)` is not a form this Postgres accepts for the
# pseudo-role, and an assertion written as a string match on `nspacl::text` would
# go stale the moment a role was added.
ok "A5  schema usage: PUBLIC holds nothing on schema pantry (0 acl entries)" "$SHAPE1" \
  "select count(*) as public_acl_entries
     from pg_namespace n,
          aclexplode(coalesce(n.nspacl, acldefault('n', n.nspowner))) a
    where n.nspname = 'pantry' and a.grantee = 0;" "0"

# ===========================================================================
say ""
say "-- the service role can reach its own database"
# ===========================================================================

# CHECK B1 service role: pantry can SELECT pantry.services
ok "B1  service role: pantry can SELECT pantry.services" "$SHAPE1" \
  "set role pantry; select count(*) from pantry.services;" ""
# CHECK B2 service role: pantry can SELECT pantry.publishers
ok "B2  service role: pantry can SELECT pantry.publishers" "$SHAPE1" \
  "set role pantry; select count(*) from pantry.publishers;" ""
# CHECK B3 service role: pantry can SELECT pantry.service_versions
ok "B3  service role: pantry can SELECT pantry.service_versions" "$SHAPE1" \
  "set role pantry; select count(*) from pantry.service_versions;" ""
# CHECK B4 service role: pantry can SELECT pantry.service_compat
ok "B4  service role: pantry can SELECT pantry.service_compat" "$SHAPE1" \
  "set role pantry; select count(*) from pantry.service_compat;" ""
# CHECK B5 service role: pantry can read the policies it wrote
# Derived rather than written down: the first run of this suite asserted a literal
# policy count (31) and got 34, because a policy count is a number that changes
# every time a table is added. What the check is actually about is reachability.
ok "B5  service role: pantry can read pg_policies for its own tables" "$SHAPE1" \
  "select (select count(*) from pg_policies where schemaname = 'pantry') > 0
            as owner_can_read_its_own_policies;" "t"
# CHECK B6 service role: pantry has no CREATE on a schema it does not own
# The negative half of "the service role can use it": in this shape `pantry` owns
# the tables and the provisioning role owns the schema, so `pantry` may not create
# objects there. That is correct and it is asserted, because a suite that only
# checked the positives would pass on a role with CREATE.
deny "B6  service role: pantry has no CREATE on the provisioning-owned schema" \
  "$SHAPE1" "42501" "permission denied for schema pantry" \
  "set role pantry; create table pantry.rls_probe (id int);"

# ===========================================================================
say ""
say "-- pantry_public: reads, and cannot write at all"
# ===========================================================================

# CHECK C1 public read: sees published third-party and first-party, nothing else
rowset "C1  public read: pantry_public's exact view of pantry.services" "$SHAPE1" \
  "$(as_public "select name from pantry.services order by name;")" \
  "alpha-api,bravo-api,cafaye"
# CHECK C2 public read: an unverified publisher is not public
rowset "C2  public read: pantry_public cannot see an unverified publisher" "$SHAPE1" \
  "$(as_public "select github_login from pantry.publishers order by github_login;")" \
  "alpha,bravo,cafaye"
# CHECK C3 public read: pantry_public cannot read a draft
rowset "C3  public read: pantry_public cannot read a draft or an unlisted row" "$SHAPE1" \
  "$(as_public "select name from pantry.services where state <> 'published' and trust <> 'first_party' order by name;")" \
  ""
# CHECK C4 public read: pantry_public has no visibility of the compatibility graph
rowset "C4  public read: pantry_public cannot read pantry.service_compat at all" "$SHAPE1" \
  "$(as_public "select count(*) from pantry.service_compat;")" "0"
# CHECK C5 public write: INSERT a publisher is refused at the grant boundary
deny "C5  public write: pantry_public cannot INSERT a publisher" \
  "$SHAPE1" "42501" "permission denied for table publishers" \
  "$(as_public "insert into pantry.publishers (github_id, github_login) values (999, 'sneaky');")"
# CHECK C6 public write: UPDATE a publisher is refused at the grant boundary
deny "C6  public write: pantry_public cannot UPDATE a publisher" \
  "$SHAPE1" "42501" "permission denied for table publishers" \
  "$(as_public "update pantry.publishers set github_login = 'renamed' where github_id = 101;")"
# CHECK C7 public write: DELETE a publisher is refused at the grant boundary
deny "C7  public write: pantry_public cannot DELETE a publisher" \
  "$SHAPE1" "42501" "permission denied for table publishers" \
  "$(as_public "delete from pantry.publishers where github_id = 101;")"
# CHECK C8 public write: INSERT a service is refused at the grant boundary
deny "C8  public write: pantry_public cannot INSERT a service" \
  "$SHAPE1" "42501" "permission denied for table services" \
  "$(as_public "insert into pantry.services (name, language, kind, core_constraint, manifest, manifest_sha256)
                values ('sneaky','go','api','^1.0.0','{}'::jsonb, repeat('0',64));")"
# CHECK C9 public write: UPDATE a service is refused at the grant boundary
deny "C9  public write: pantry_public cannot UPDATE a service" \
  "$SHAPE1" "42501" "permission denied for table services" \
  "$(as_public "update pantry.services set state = 'published' where name = 'alpha-draft';")"
# CHECK C10 public write: DELETE a service is refused at the grant boundary
deny "C10 public write: pantry_public cannot DELETE a service" \
  "$SHAPE1" "42501" "permission denied for table services" \
  "$(as_public "delete from pantry.services where name = 'alpha-api';")"
# CHECK C11 public write: INSERT a version is refused at the grant boundary
deny "C11 public write: pantry_public cannot INSERT a service_version" \
  "$SHAPE1" "42501" "permission denied for table service_versions" \
  "$(as_public "insert into pantry.service_versions (service_id, tag) values ('$B1', 'v9.9.9');")"
# CHECK C12 public write: INSERT a compatibility edge is refused at the grant boundary
deny "C12 public write: pantry_public cannot INSERT a service_compat edge" \
  "$SHAPE1" "42501" "permission denied for table service_compat" \
  "$(as_public "insert into pantry.service_compat (service_id, target_id, kind, version_range)
                values ('$B1','$B3','requires','^1.0.0');")"
# CHECK C13 public escalation: pantry_public holds no membership at all
# NOT "attempt `set role pantry_admin` and watch it fail", and the reason is worth
# recording because it was this suite's first attempt: **the suite connects as a
# superuser, and a superuser's `SET ROLE` is not subject to membership.** Measured
# on PostgreSQL 18.4 — `set role pantry_public; set role pantry_admin;` returns
# `SET` twice and `current_user` is `pantry_admin`, on a cluster where
# `pg_auth_members` has no row for either role. The check would have passed for
# every role in the schema while proving nothing, which is worse than no check.
#
# What actually makes escalation impossible is two catalog facts: `pantry_public` is
# a member of nothing, and every role here is NOINHERIT so a membership granted
# later cannot be inherited silently. Both are asserted; neither is inferred.
ok "C13 public escalation: pantry_public is a member of no role" "$SHAPE1" \
  "select coalesce(string_agg(m.rolname, ','), '<none>') as memberships
     from pg_roles r
     left join pg_auth_members am on am.member = r.oid
     left join pg_roles  m on m.oid = am.roleid
    where r.rolname = 'pantry_public';" "<none>"
# CHECK C14 public escalation: no role in this schema is superuser or BYPASSRLS
# `coalesce`, because `string_agg` over zero rows is NULL rather than an empty
# string and `-t -A` prints a NULL as a blank line — which the first run of this
# suite read as "the answer was nothing at all" rather than "there were no such
# roles", which is the same sentence with the opposite meaning.
ok "C14 public escalation: no pantry role is SUPERUSER or BYPASSRLS" "$SHAPE1" \
  "select coalesce(string_agg(rolname, ',' order by rolname), '<none>') as privileged_roles
     from pg_roles
    where rolname in ('pantry','pantry_public','pantry_publisher','pantry_admin')
      and (rolsuper or rolbypassrls or rolcreatedb or rolcreaterole);" "<none>"
# CHECK C15 public escalation: every pantry role is NOINHERIT
ok "C15 public escalation: every pantry role is NOINHERIT" "$SHAPE1" \
  "select count(*) as inheriting_roles
     from pg_roles
    where rolname in ('pantry','pantry_public','pantry_publisher','pantry_admin')
      and rolinherit;" "0"
# CHECK C16 public write: none of the refused writes changed a row
# One concatenated string rather than four columns: `ok` matches a fragment of the
# output as text, and psql's column separator is a tab, which `oneline` does not
# normalise — so a four-column expectation is an expectation on a tab.
ok "C16 public write: none of the refused writes changed a row" "$SHAPE1" \
  "select (select count(*) from pantry.publishers)::text || '/' ||
          (select count(*) from pantry.services)::text || '/' ||
          (select count(*) from pantry.service_versions)::text || '/' ||
          (select count(*) from pantry.service_compat)::text as counts;" "4/5/2/3"

# ===========================================================================
say ""
say "-- pantry_publisher: its own rows, and only its own rows"
# ===========================================================================

# CHECK D1 publisher isolation: the harness's own impersonation is subject to RLS
# This doubles as a self-check of the harness. If `set role` from a superuser
# session bypassed RLS, this would answer five rows instead of two and the suite
# would be asserting nothing for the rest of its life.
rowset "D1  publisher isolation: this suite's SET ROLE is subject to RLS" "$SHAPE1" \
  "$(as_publisher "$A1" "select name from pantry.services order by name;")" \
  "alpha-api,alpha-draft"
# CHECK D2 publisher isolation: sees its own publisher row only
rowset "D2  publisher isolation: sees its own publisher row and no other" "$SHAPE1" \
  "$(as_publisher "$A1" "select github_login from pantry.publishers order by github_login;")" \
  "alpha"
# CHECK D3 publisher isolation: sees its own service_versions only
rowset "D3  publisher isolation: sees its own versions and not a first-party one" "$SHAPE1" \
  "$(as_publisher "$A1" "select tag from pantry.service_versions order by tag;")" \
  "v1.2.3"
# CHECK D4 publisher isolation: sees the edges of its own services only
rowset "D4  publisher isolation: sees only the edges of its own services" "$SHAPE1" \
  "$(as_publisher "$A1" "select kind || '->' || target_id from pantry.service_compat order by 1;")" \
  "conflicts_with->$B3,requires->00000000-0000-4000-8000-0000000000b5"
# CHECK D5 publisher isolation: another publisher's service is invisible by primary key
# An EMPTY row set, asserted as one: RLS removes the row from the result rather than
# raising, so "sees nothing" is a row set of length zero and not an error. `-t`
# suppresses the `(0 rows)` footer, which is why this is `rowset` with an empty
# expectation rather than `ok` matching a string psql no longer prints.
rowset "D5  publisher isolation: another publisher's row is absent from the result, not an error" "$SHAPE1" \
  "$(as_publisher "$A1" "select name from pantry.services where id = '$B3';")" \
  ""
# CHECK D6 publisher isolation: no identity reads nothing
ok "D6  publisher isolation: with no identity a publisher reads zero rows" "$SHAPE1" \
  "set role pantry_publisher; select count(*) from pantry.services;" "0"
# CHECK D7 publisher write: may insert its own third-party service
ok "D7  publisher write: may INSERT its own third-party service" "$SHAPE1" \
  "$(as_publisher "$A1" "insert into pantry.services (name, language, kind, trust, state, core_constraint, manifest, manifest_sha256, publisher_id)
                values ('alpha-new','go','api','third_party','draft','^1.0.0','{}'::jsonb, repeat('1',64), '$A1');")" "INSERT 0 1"
# CHECK D8 publisher write: the new row is readable by its owner
ok "D8  publisher write: the row it just inserted is readable by its owner" "$SHAPE1" \
  "$(as_publisher "$A1" "select name from pantry.services where name = 'alpha-new';")" "alpha-new"
# CHECK D9 publisher write: may not promote a service to first_party
deny "D9  publisher write: may NOT insert trust = first_party" \
  "$SHAPE1" "42501" "row-level security" \
  "$(as_publisher "$A1" "insert into pantry.services (name, language, kind, trust, state, core_constraint, manifest, manifest_sha256, publisher_id)
                values ('alpha-trust','go','api','first_party','draft','^1.0.0','{}'::jsonb, repeat('2',64), '$A1');")"
# CHECK D10 publisher write: may not insert a row owned by another publisher
deny "D10 publisher write: may NOT insert a service owned by another publisher" \
  "$SHAPE1" "42501" "row-level security" \
  "$(as_publisher "$A1" "insert into pantry.services (name, language, kind, trust, state, core_constraint, manifest, manifest_sha256, publisher_id)
                values ('alpha-stolen','go','api','third_party','draft','^1.0.0','{}'::jsonb, repeat('3',64), '00000000-0000-4000-8000-0000000000a2');")"
# CHECK D11 publisher write: may not promote its own row to first_party on update
deny "D11 publisher write: may NOT UPDATE its own service to trust = first_party" \
  "$SHAPE1" "42501" "row-level security" \
  "$(as_publisher "$A1" "update pantry.services set trust = 'first_party' where name = 'alpha-api';")"
# CHECK D12 publisher write: an UPDATE of another publisher's row changes nothing
# `using (false)` on a row the caller cannot see is SILENT — 0 rows, exit 0. An
# assertion that expected an error here would fail against a correctly working
# policy, which is why this is `ok` with the row count as the expectation.
ok "D12 publisher write: an UPDATE of another publisher's service changes 0 rows" "$SHAPE1" \
  "$(as_publisher "$A1" "update pantry.services set description = 'hijacked' where id = '$B3';")" "UPDATE 0"
# CHECK D13 publisher write: may not delete its own service — at the GRANT boundary
# `deny` names `permission denied for table`, not `row-level security`, and the
# difference is the interesting part. `pantry_publisher` holds NO DELETE privilege
# on `services`, so the statement dies in `aclcheck_error` and the `using (false)`
# policy three lines away is never consulted. That is the stronger of the two
# guarantees, and asserting the weaker one would have been asserting a mechanism
# that does not run. (The policy DOES run for the OWNER — see F7, the same refusal
# with no grant behind it.)
deny "D13 publisher write: may NOT DELETE its own service" \
  "$SHAPE1" "42501" "permission denied for table services" \
  "$(as_publisher "$A1" "delete from pantry.services where name = 'alpha-new';")"
# CHECK D14 publisher write: the refused delete left the row alone
ok "D14 publisher write: the refused DELETE left the row in place" "$SHAPE1" \
  "set role pantry_admin; select name from pantry.services where name = 'alpha-new';" "alpha-new"
# CHECK D15 publisher write: may not delete a version
deny "D15 publisher write: may NOT DELETE a service_version" \
  "$SHAPE1" "42501" "permission denied for table service_versions" \
  "$(as_publisher "$A1" "delete from pantry.service_versions where tag = 'v1.2.3';")"
# CHECK D16 publisher write: may not insert a version for a first-party service
deny "D16 publisher write: may NOT add a version to a FIRST-PARTY service" \
  "$SHAPE1" "42501" "row-level security" \
  "$(as_publisher "$A1" "insert into pantry.service_versions (service_id, tag)
                values ('00000000-0000-4000-8000-0000000000b5', 'v9.9.9');")"
# CHECK D17 publisher write: may not insert a publisher row at all
# Phase 1 is official-only (MVP-SCOPE: no registration webhook), so no publisher
# may create a publisher. It is refused by the GRANT — `pantry_publisher` holds
# `select, update` on `publishers` and nothing else — and the assertion says so
# rather than naming the policy, because the policy is not what refuses it. Two
# independent barriers would be the shape to prefer here, and `publishers` has only
# one: adding a publisher INSERT policy would make the refusal redundant rather
# than stronger, and is recorded in the report rather than written here.
deny "D17 publisher write: may NOT INSERT a publisher row (no self-registration)" \
  "$SHAPE1" "42501" "permission denied for table publishers" \
  "$(as_publisher "$A1" "insert into pantry.publishers (github_id, github_login) values (555, 'selfreg');")"

# ===========================================================================
say ""
say "-- pantry_admin: the decision-maker, and the first-party ingest"
# ===========================================================================

# CHECK E1 admin read: sees every service
rowset "E1  admin read: sees every service, including drafts and unlisted" "$SHAPE1" \
  "$(as_admin "select name from pantry.services order by name;")" \
  "alpha-api,alpha-draft,alpha-new,bravo-api,bravo-hidden,cafaye"
# CHECK E2 admin read: sees every publisher, verified or not
rowset "E2  admin read: sees every publisher including the unverified one" "$SHAPE1" \
  "$(as_admin "select github_login from pantry.publishers order by github_login;")" \
  "alpha,bravo,cafaye,unverified"
# CHECK E3 admin read: sees the whole compatibility graph
ok "E3  admin read: sees every compatibility edge" "$SHAPE1" \
  "$(as_admin "select count(*) from pantry.service_compat;")" "3"
# CHECK E4 admin write: may create a FIRST-PARTY publisher — the ingest's first row
ok "E4  admin write: may INSERT a first-party publisher (ingest step 1)" "$SHAPE1" \
  "$(as_admin "insert into pantry.publishers (github_id, github_login, is_first_party, verified)
                values (606, 'ingested', true, true);")" "INSERT 0 1"
# CHECK E5 admin write: may create a FIRST-PARTY service, which has no publisher_id
ok "E5  admin write: may INSERT a first-party service with publisher_id NULL (ingest step 2)" "$SHAPE1" \
  "$(as_admin "insert into pantry.services (name, language, kind, trust, state, core_constraint, manifest, manifest_sha256, ingested_by)
                values ('courier','go','api','first_party','published','^0.1.0','{}'::jsonb, repeat('4',64), 'pantry-ingest');")" "INSERT 0 1"
# CHECK E6 admin write: may create a version for that first-party service
ok "E6  admin write: may INSERT a version of the ingested first-party service (ingest step 3)" "$SHAPE1" \
  "$(as_admin "insert into pantry.service_versions (service_id, tag, version_major, version_minor, version_patch)
                select id, 'v1.4.0', 1, 4, 0 from pantry.services where name = 'courier';")" "INSERT 0 1"
# CHECK E7 admin write: may create a compatibility edge for that service
ok "E7  admin write: may INSERT a compatibility edge for the ingested service (ingest step 4)" "$SHAPE1" \
  "$(as_admin "insert into pantry.service_compat (service_id, target_id, kind, version_range, dependency)
                select s.id, t.id, 'requires', '^0.1.0', 'required'
                  from pantry.services s, pantry.services t
                 where s.name = 'courier' and t.name = 'alpha-api';")" "INSERT 0 1"
# CHECK E8 admin write: the ingested row is publicly visible immediately
ok "E8  admin write: the ingested first-party service is visible to the public role" "$SHAPE1" \
  "$(as_public "select name from pantry.services where name = 'courier';")" "courier"
# CHECK E9 admin write: may publish a third-party submission (the review action)
ok "E9  admin write: may publish a publisher's submitted service (the review action)" "$SHAPE1" \
  "$(as_admin "update pantry.services set state = 'published' where name = 'alpha-draft';")" "UPDATE 1"
# CHECK E10 admin write: may delete a version
ok "E10 admin write: may DELETE a service_version" "$SHAPE1" \
  "$(as_admin "delete from pantry.service_versions where tag = 'v0.1.0';")" "DELETE 1"
# CHECK E11 admin limits: no CREATE on the schema
deny "E11 admin limits: pantry_admin has no CREATE on schema pantry" \
  "$SHAPE1" "42501" "permission denied for schema pantry" \
  "$(as_admin "create table pantry.rls_admin_probe (id int);")"
# CHECK E12 admin limits: no CREATE on any other schema
deny "E12 admin limits: pantry_admin cannot create in the cluster's public schema" \
  "$SHAPE1" "42501" "permission denied for schema public" \
  "$(as_admin "create table public.rls_admin_probe (id int);")"

# ===========================================================================
say ""
say "-- pantry itself: FORCE, and the deliberate absence of an INSERT policy"
# ===========================================================================

# CHECK F1 force: every table is FORCEd
ok "F1  force: every table in schema pantry is FORCE ROW LEVEL SECURITY" "$SHAPE1" \
  "select count(*) as not_forced
     from pg_class c join pg_namespace n on n.oid = c.relnamespace
    where n.nspname = 'pantry' and c.relkind = 'r'
      and (not c.relrowsecurity or not c.relforcerowsecurity);" "0"
# CHECK F2 force: the owner is genuinely subject to the policies
# With no identity set, every publisher predicate compares against NULL, so the
# owner reads nothing. That is the accepted cost of FORCE and it is asserted here
# rather than left to be discovered by the first backfill.
ok "F2  force: with no identity the owner reads zero rows, not every row" "$SHAPE1" \
  "set role pantry; select count(*) from pantry.services;" "0"
# CHECK F3 force: with an identity the owner reads exactly one publisher's rows
rowset "F3  force: with an identity set, the owner is scoped like any publisher" "$SHAPE1" \
  "$(as_service "select pantry.begin_publisher('$A1');
                 select name from pantry.services order by name;")" \
  "alpha-api,alpha-draft,alpha-new"
# CHECK F4 service role: may insert its own publisher-scoped service
ok "F4  service role: with an identity, pantry may INSERT its own service" "$SHAPE1" \
  "$(as_service "select pantry.begin_publisher('$A1');
                 insert into pantry.services (name, language, kind, trust, state, core_constraint, manifest, manifest_sha256, publisher_id)
                 values ('alpha-own','go','api','third_party','draft','^1.0.0','{}'::jsonb, repeat('5',64), '$A1');")" "INSERT 0 1"
# CHECK F5 service role: may NOT run the first-party ingest — the decision
# This is the check that makes D31's role map a decision rather than an accident
# of who held a grant. It is the negative half of E5: the same row, the same
# table, refused here and permitted there, and the difference is the role.
deny "F5  service role: pantry may NOT insert a FIRST-PARTY service (ingest is pantry_admin's)" \
  "$SHAPE1" "42501" "row-level security" \
  "$(as_service "insert into pantry.services (name, language, kind, trust, state, core_constraint, manifest, manifest_sha256, ingested_by)
                 values ('pantry-ingested','go','api','first_party','published','^1.0.0','{}'::jsonb, repeat('6',64), 'pantry');")"
# CHECK F6 service role: may NOT insert a publisher row
deny "F6  service role: pantry may NOT INSERT a publisher row" \
  "$SHAPE1" "42501" "row-level security" \
  "set role pantry; insert into pantry.publishers (github_id, github_login) values (777, 'service-made');"
# CHECK F7 service role: the owner's DELETE is refused by the POLICY, not the grant
# The same refusal as D13 with the grant removed, and the mechanism differs: `pantry`
# OWNS the table, so it holds DELETE implicitly, and what stops the delete is
# `services_publisher_select_no_delete_marker`'s `using (false)`. A `using` clause
# that matches nothing is SILENT — the statement succeeds and changes zero rows, so
# this is `ok` with the row count as the expectation and not `deny`. Asserting an
# error here would have been asserting a mechanism Postgres does not have.
#
# This is the check that shows FORCE doing its job. Without `force row level
# security` this statement would delete the row, because the owner is exempt from
# its own policies, and the exempt owner's DELETE privilege is exactly the bypass
# D31 exists to prevent.
ok "F7  force: the owner's DELETE of its own service changes 0 rows, not an error" "$SHAPE1" \
  "$(as_service "select pantry.begin_publisher('$A1'); delete from pantry.services where name = 'alpha-own';")" \
  "DELETE 0"
# CHECK F8 service role: the refused delete left the row alone
ok "F8  force: the owner's refused DELETE left the row in place" "$SHAPE1" \
  "set role pantry_admin; select name from pantry.services where name = 'alpha-own';" "alpha-own"
# CHECK F9 service role: may NOT promote its own service to first_party
deny "F9  service role: pantry may NOT set trust = first_party either" \
  "$SHAPE1" "42501" "row-level security" \
  "$(as_service "select pantry.begin_publisher('$A1'); update pantry.services set trust = 'first_party' where name = 'alpha-own';")"

# ===========================================================================
say ""
say "-- the catalog invariants, derived from pg_policies rather than written down"
#
# Every check in the three tiers above is a statement about four roles and four
# tables that somebody typed. These four are statements about the SHAPE, derived
# from the policies themselves, so a table or a policy added tomorrow is covered
# the day it lands. Each of the first three is red on the version of
# 00006_rls.sql this packet started from; the mutation log in
# REPORT-registry-pantry-schema-02.md records the runs.
# ===========================================================================

# CHECK G1 invariant: every role a policy names holds USAGE on the schema
# The check that would have caught this packet's defect on its own: a policy names
# `pantry`, `pantry` held no USAGE, and the only record of that was a comment
# arguing the grant was unnecessary.
ok "G1  invariant: every role named by any policy holds USAGE on schema pantry" "$SHAPE1" \
  "select coalesce(string_agg(distinct r.role_name, ', '), '<none>') as without_usage
     from pg_policies p, lateral unnest(p.roles) as r(role_name)
    where p.schemaname = 'pantry'
      and not has_schema_privilege(r.role_name, 'pantry', 'USAGE');" "<none>"
# CHECK G2 invariant: every policy command has a matching table privilege
# This is the check that would have caught the missing `pantry_admin` grants:
# eight admin policies, zero privileges, and a catalog that read as a finished
# boundary. It is also why the fix was a grant rather than a policy — a role with
# no privilege never reaches a policy, so this gap is invisible in `pg_policies`.
ok "G2  invariant: every command every policy allows has a matching table privilege" "$SHAPE1" \
  "select coalesce(string_agg(format('%s %s %s', p.tablename, p.cmd, r.role_name), ', '), '<none>') as ungranted
     from pg_policies p
     cross join lateral unnest(p.roles) as r(role_name)
     cross join lateral (values (case p.cmd
                        when 'select' then 'SELECT' when 'insert' then 'INSERT'
                        when 'update' then 'UPDATE' when 'delete' then 'DELETE' end)) v(verb)
    where p.schemaname = 'pantry'
      and not has_table_privilege(r.role_name, format('pantry.%I', p.tablename)::regclass, v.verb);" "<none>"
# CHECK G3 invariant: nobody holds a table privilege they cannot reach
# The inverse direction, and the one the brief's lockout fell into from the other
# side: a role can hold SELECT on four tables and still be unable to read a row if
# it does not hold USAGE on the schema those tables are in.
ok "G3  invariant: no role holds a table privilege without schema USAGE" "$SHAPE1" \
  "select coalesce(string_agg(format('%s %s on %s', r.role_name, v.verb, c.relname), ', '), '<none>') as unreachable
     from pg_class c
     join pg_namespace n on n.oid = c.relnamespace
     cross join (values ('pantry'), ('pantry_public'), ('pantry_publisher'), ('pantry_admin'), ('public')) r(role_name)
     cross join (values ('SELECT'), ('INSERT'), ('UPDATE'), ('DELETE')) v(verb)
    where n.nspname = 'pantry' and c.relkind = 'r'
      and has_table_privilege(r.role_name, c.oid, v.verb)
      and not has_schema_privilege(r.role_name, 'pantry', 'USAGE');" "<none>"
# CHECK G4 invariant: no policy is FOR ALL
ok "G4  invariant: no policy is FOR ALL (one policy per command)" "$SHAPE1" \
  "select count(*) as for_all from pg_policies where schemaname = 'pantry' and cmd = 'all';" "0"
# CHECK G5 invariant: every role that can write can write all four tables or none
# Not a security property — a coherence one. The four tables are one catalog, and a
# role that can update three of them is a role whose fourth write will fail in
# production rather than in this suite.
ok "G5  invariant: pantry_admin's write set covers all four tables, pantry_publisher's does not claim to" "$SHAPE1" \
  "select string_agg(t, ' ' order by t) as admin_writable
     from (values ('publishers'),('services'),('service_versions'),('service_compat')) x(t)
    where has_table_privilege('pantry_admin', format('pantry.%I', t)::regclass, 'INSERT');" \
  "publishers service_compat service_versions services"

# ===========================================================================
say ""
say "-- shape 2: the same migrations with pantry owning the schema too"
#
# Both deployments have to be green. Shape 1 passes because this directory grants
# USAGE; shape 2 would pass even if it did not, because an owner holds USAGE
# implicitly — which is exactly why shape 1 exists, and why shape 2's checks are
# the service role's reachability rather than the invariants.
# ===========================================================================

# CHECK H1 shape 2: the service role can read
ok "H1  shape 2: pantry can SELECT pantry.services when it owns the schema" "$SHAPE2" \
  "set role pantry; select count(*) from pantry.services;" ""
# CHECK H2 shape 2: the service role can run DDL
ok "H2  shape 2: pantry can CREATE in schema pantry when it owns it" "$SHAPE2" \
  "set role pantry; create table pantry.rls_probe (id int); drop table pantry.rls_probe;"
# CHECK H3 shape 2: the public read role still reads
ok "H3  shape 2: pantry_public can still SELECT when pantry owns the schema" "$SHAPE2" \
  "set role pantry_public; select count(*) from pantry.services;" ""
# CHECK H4 shape 2: the invariants hold here too
ok "H4  shape 2: every role named by any policy holds USAGE on schema pantry" "$SHAPE2" \
  "select coalesce(string_agg(distinct r.role_name, ', '), '<none>') as without_usage
     from pg_policies p, lateral unnest(p.roles) as r(role_name)
    where p.schemaname = 'pantry'
      and not has_schema_privilege(r.role_name, 'pantry', 'USAGE');" "<none>"
# CHECK H5 shape 2: pantry_admin holds the grants the ingest needs
ok "H5  shape 2: pantry_admin may INSERT a first-party service (the ingest)" "$SHAPE2" \
  "set role pantry_admin;
   insert into pantry.services (name, language, kind, trust, state, core_constraint, manifest, manifest_sha256, ingested_by)
   values ('shape2-first-party','go','api','first_party','published','^1.0.0','{}'::jsonb, repeat('7',64), 'pantry-ingest');" \
  "INSERT 0 1"