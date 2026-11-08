# ADR 0015: Database-independent connections over registered drivers

- **Status:** accepted
- **Date:** 2026-11-07

## Context

Phase 7 implemented parameterized SQL, least-privilege roles, write approvals,
auditing, and reviewed migrations over SQLite. It also carried a PostgreSQL
target policy and `AGENTS.md` §10 required a PostgreSQL release-CI test. That
policy parsed URLs and refused private hosts, but no wire adapter existed, so it
created a release gate the project could not honestly satisfy.

Axiom's database surface is already vendor-neutral at the language level:
application code calls `db.query`/`db.query_one`/`db.execute` with a named
database and parameterized SQL. The named connection comes from
`axiom.yaml`, not from source. The missing piece was a configuration model that
treats the backend as a pluggable connection rather than a fixed engine enum
with one required remote vendor.

## Decision

1. A database is a **named connection plus policy** in `axiom.yaml`. Multiple
   connections may be declared; `.ax` code selects one by name
   (`database: "app"`), so business logic stays driver-independent.
2. The connection is described by a `driver` name, a local `path` or a remote
   `url`, and the shared policy fields (`data_class`, `allowed_tables`,
   `max_rows`, `timeout_seconds`, `migrations`, `destructive_migrations`,
   `allow_private_network`, `require_write_approval`).
3. `sqlite` is the bundled reference driver. Other drivers are host-registered
   adapters behind the same typed `db.*` API; the grammar does not change when a
   driver is added.
4. The configuration layer validates the shape of a connection and the safety of
   local paths. It does not hard-code vendor names. `axiom db check` and the
   runtime report a driver that is not registered in the current host.
5. The PostgreSQL target policy, the `postgres` engine enum, and the PostgreSQL
   release gate are removed. There is no PostgreSQL wire adapter requirement for
   1.0. A future PostgreSQL adapter may be added as an optional driver.
6. Drivers remain server-side only. Browser-target code never receives database
   connections, credentials, or driver code.

## Consequences

- `DatabaseConfig.engine` becomes `DatabaseConfig.driver`; examples, fixtures,
  CLI output, and tests use `driver: sqlite`.
- `axiom-database` no longer exports `PostgresPolicy`/`PostgresTarget`; the
  `postgres` module and its tests are deleted.
- `ax` source is unchanged: the same `db.query<Row>(database: "...", sql: ...)`
  calls work across drivers.
- Release readiness no longer lists a PostgreSQL wire adapter as a blocker. The
  open requirement becomes "the bundled SQLite driver and the documented driver
  interface", which is what the repository actually tests.
- Adding a driver means implementing the host adapter and registering it; it does
  not add a language declaration, a config keyword per vendor, or a second
  validation path.
