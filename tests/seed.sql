-- seed.sql — the served catalog.
--
-- WHY THIS FILE EXISTS AND WHY IT IS NOT THE RLS SUITE'S FIXTURE. The RLS
-- suite's fixtures live in `tests/rls_checks.sh` and are deliberately abstract:
-- `'{}'::jsonb` manifests and names chosen so an expectation can be read as a
-- literal. This is the OTHER kind of seed — a catalog a client would render,
-- with real manifests in `services.manifest`, because the thing being proven is
-- that `GET /v1/services` answers 200 with objects the document can serialise.
--
-- It is written for `tests/rls.sh --serve`, which is what the Go tests in
-- `internal/pantrydb` and `internal/httpapi` start their PostgreSQL through. One
-- seed, two consumers; the Go suite does not carry its own.
--
-- IT IS SEEDED AS `pantry_admin`, which is D32's answer and not a convenience:
-- `pantry` has no INSERT policy by design (`00006_rls.sql` argues it at
-- length), so the first-party rows below — every one of which has
-- `publisher_id IS NULL` because `00003`'s CHECK forbids otherwise — can only be
-- written by the role that exists to make that decision.
--
-- THE ROWS ARE CHOSEN TO MAKE THREE CLAIMS TESTABLE, and every one of the claims
-- is about something the QUERY does not do:
--
--   identity / courier / muse / caf   the four first-party services. `caf` is a
--     `cli` with `base_path NULL`, which is the shape that proves a null in the
--     response is a fact ("this publishes no OpenAPI document") rather than a
--     missing field.
--   parcel                          a third-party service on a VERIFIED publisher,
--     so the public catalog contains something a publisher owns without the role
--     being anything but `pantry_public`.
--   draft-only, unlisted-one         two rows the public read must NOT see. Their
--     presence is the point: if they show up in a response, something wrote a
--     `where state = 'published'` that the RLS policy was supposed to own, and
--     this file is how that is caught.
set role pantry_admin;

insert into pantry.publishers (id, github_id, github_login, is_first_party, verified) values
  ('00000000-0000-4000-8000-0000000000a1', 101, 'alpha',      false, true),
  ('00000000-0000-4000-8000-0000000000a2', 202, 'unverified', false, false);

-- The four first-party services. `publisher_id` is NULL and `trust` is
-- `first_party` on every one, because `00003`'s
-- `services_first_party_has_no_publisher` CHECK makes any other combination
-- uninsertable rather than merely discouraged.
insert into pantry.services
  (name, description, language, kind, trust, state, core_constraint, base_path,
   manifest, manifest_sha256, source_repo, source_ref, ingested_by, publisher_id)
values
  ('identity',
   'Authentication, sessions, MFA, OAuth, accounts and tenancy, and the OIDC provider.',
   'go', 'api', 'first_party', 'published', '^0.1.0', '/v1',
   '{"name":"identity","description":"Authentication, sessions, MFA, OAuth, accounts and tenancy, and the OIDC provider.","language":"go","core":"^0.1.0","exposes":{"api":"openapi/v1.yaml","events":["identity.user.created"]},"consumes":[],"dependencies":[],"repository":{"url":"git@github.com:cafaye/identity.git","defaultBranch":"master","visibility":"public"},"owner":{"team":"identity","contact":"identity@cafaye.com"}}'::jsonb,
   repeat('1', 64), 'git@github.com:cafaye/identity.git', 'HEAD', 'tests/seed.sql', NULL),

  ('courier',
   'Outbound deliveries: email, SMS and push, retried and deduplicated.',
   'go', 'api', 'first_party', 'published', '^0.1.0', '/v1',
   '{"name":"courier","description":"Outbound deliveries: email, SMS and push, retried and deduplicated.","language":"go","core":"^0.1.0","exposes":{"api":"openapi/v1.yaml","events":["courier.message.queued"]},"consumes":["identity.user.created"],"dependencies":[{"name":"identity","version":"^0.1.0","required":true}],"repository":{"url":"git@github.com:cafaye/courier.git","defaultBranch":"master","visibility":"public"},"owner":{"team":"courier","contact":"courier@cafaye.com"}}'::jsonb,
   repeat('2', 64), 'git@github.com:cafaye/courier.git', 'HEAD', 'tests/seed.sql', NULL),

  ('muse',
   'The cafaye product: sessions, plans, and the billing surface.',
   'typescript', 'api', 'first_party', 'published', '^0.2.0', '/v1',
   '{"name":"muse","description":"The cafaye product: sessions, plans, and the billing surface.","language":"typescript","core":"^0.2.0","exposes":{"api":"openapi/v1.yaml","events":["muse.plan.subscribed"]},"consumes":["identity.user.created","courier.message.queued"],"dependencies":[{"name":"identity","version":"^0.1.0","required":true},{"name":"courier","version":"^0.1.0","required":false}],"repository":{"url":"git@github.com:cafaye/muse.git","defaultBranch":"master","visibility":"private"},"owner":{"team":"muse","contact":"muse@cafaye.com"}}'::jsonb,
   repeat('3', 64), 'git@github.com:cafaye/muse.git', 'HEAD', 'tests/seed.sql', NULL),

  -- A `cli`, which is a manifest that declares no contract surface: `exposes` is
  -- NULL, not `{}`, and `base_path` is NULL because a binary publishes no OpenAPI
  -- document. Both nulls are load-bearing in the response.
  ('caf',
   'The cafaye CLI: a developer''s entry point to the platform.',
   'rust', 'cli', 'first_party', 'published', '^0.1.0', NULL,
   '{"name":"caf","description":"The cafaye CLI: a developer''s entry point to the platform.","language":"rust","core":"^0.1.0","exposes":null,"consumes":[],"dependencies":[],"repository":{"url":"git@github.com:cafaye/caf.git","defaultBranch":"master","visibility":"public"},"owner":{"team":"caf","contact":"caf@cafaye.com"}}'::jsonb,
   repeat('4', 64), 'git@github.com:cafaye/caf.git', 'HEAD', 'tests/seed.sql', NULL);

-- A third-party service, on a VERIFIED publisher. This is the row that proves the
-- catalog is not a first-party-only view.
insert into pantry.services
  (name, description, language, kind, trust, state, core_constraint, base_path,
   manifest, manifest_sha256, source_repo, source_ref, ingested_by, publisher_id)
values
  ('parcel',
   'A third-party registry client, published by a verified publisher.',
   'go', 'api', 'third_party', 'published', '^0.2.0', '/v1',
   '{"name":"parcel","description":"A third-party registry client, published by a verified publisher.","language":"go","core":"^0.2.0","exposes":{"api":"openapi/v1.yaml","events":[]},"consumes":[],"dependencies":[{"name":"courier","version":"^0.1.0","required":true}],"repository":{"url":"git@github.com:example/parcel.git","defaultBranch":"master","visibility":"public"},"owner":{"team":"parcel","contact":"parcel@example.com"}}'::jsonb,
   repeat('5', 64), 'git@github.com:example/parcel.git', 'HEAD', 'tests/seed.sql',
   '00000000-0000-4000-8000-0000000000a1');

-- THE TWO ROWS THE PUBLIC READ MUST NOT SEE. They exist in the seed precisely so
-- that their absence is measured rather than assumed.
insert into pantry.services
  (name, description, language, kind, trust, state, core_constraint, base_path,
   manifest, manifest_sha256, ingested_by, publisher_id)
values
  ('draft-only',
   'A third-party draft. Visible to its own publisher and to nobody else.',
   'go', 'api', 'third_party', 'draft', '^0.2.0', '/v1',
   '{"name":"draft-only","description":"A third-party draft. Visible to its own publisher and to nobody else.","language":"go","core":"^0.2.0","exposes":{"api":"openapi/v1.yaml","events":[]},"consumes":[],"dependencies":[],"repository":{"url":"git@github.com:example/draft-only.git","defaultBranch":"master","visibility":"public"},"owner":{"team":"example","contact":"example@example.com"}}'::jsonb,
   repeat('6', 64), 'tests/seed.sql', '00000000-0000-4000-8000-0000000000a1'),
  ('unlisted-one',
   'Published and then taken off the listing. The row stays; the listing does not.',
   'go', 'api', 'third_party', 'unlisted', '^0.2.0', '/v1',
   '{"name":"unlisted-one","description":"Published and then taken off the listing. The row stays; the listing does not.","language":"go","core":"^0.2.0","exposes":{"api":"openapi/v1.yaml","events":[]},"consumes":[],"dependencies":[],"repository":{"url":"git@github.com:example/unlisted-one.git","defaultBranch":"master","visibility":"public"},"owner":{"team":"example","contact":"example@example.com"}}'::jsonb,
   repeat('7', 64), 'tests/seed.sql', '00000000-0000-4000-8000-0000000000a1');

-- Real git tags, stored verbatim, with a parsed triple beside them. Nothing in
-- the HTTP read path reads this table yet — MVP-SCOPE says versions come from the
-- publisher's real tags and not from an invented scheme — and it is seeded
-- because a catalog with no versions is not the catalog this schema describes.
insert into pantry.service_versions (service_id, tag, version_major, version_minor, version_patch, released_at)
select id, v.tag, v.major, v.minor, v.patch, v.released
  from pantry.services s
  join (values
    ('identity', 'v1.2.3', 1, 2, 3,  '2025-11-04'::timestamptz),
    ('identity', 'v1.3.0', 1, 3, 0,  '2026-02-17'::timestamptz),
    ('courier',  'v0.4.1', 0, 4, 1,  '2026-01-09'::timestamptz),
    ('muse',     'v2.0.0', 2, 0, 0,  '2026-06-01'::timestamptz),
    ('caf',      'v0.9.0', 0, 9, 0,  '2026-03-22'::timestamptz),
    ('parcel',   'v0.1.0', 0, 1, 0,  '2026-08-30'::timestamptz)
  ) as v(name, tag, major, minor, patch, released)
    on v.name = s.name;

-- THE COMPATIBILITY GRAPH, and this is the row that answers the question the
-- registry exists for: courier cannot run without identity, at `^0.1.0`, and
-- muse cannot run without courier. Two directed edges out of the fleet's centre.
--
-- `dependency` is 'required' rather than 'soft' for both, so they are the two
-- edges a recursive traversal answers for.
insert into pantry.service_compat (service_id, target_id, kind, version_range, dependency, source)
select a.id, b.id, 'requires', v.range, v.dependency, 'manifest'
  from (values
    ('courier',  'identity', '^0.1.0', 'required'),
    ('muse',     'identity', '^0.1.0', 'required'),
    ('muse',     'courier',  '^0.1.0', 'required'),
    ('parcel',   'courier',  '^0.1.0', 'soft')
  ) as v(from_name, to_name, range, dependency)
  join pantry.services a on a.name = v.from_name
  join pantry.services b on b.name = v.to_name;

-- Back to the provisioning role for anything that follows, so a `--serve` client
-- that wants to write is not left holding `pantry_admin`.
reset role;
