# Catalog & Discovery Service

Implements SDD-2026-001 v1.1 §5.2 / §6.2 / §6.4* / §6.5 / §7.2 — project
metadata, versioning, yank/un-yank, and discovery/search, sitting
alongside `authnz` (Identity) in the platform described in
RS-2026-001 v1.1.

*§6.4 (compliance export/delete) is Admin & Audit's endpoint, not
Catalog's, but Catalog is one of the services it coordinates with when
fulfilling a deletion — see "Open items" below.

## How it fits the platform

Per SDD §3.1/§3.2, Catalog:

- Is reached only through the API Gateway, never directly by clients.
- Trusts `X-User-Id` / `X-User-Email` / `X-User-Name` set by the
  gateway on every request (`src/identity.rs`) — this is the same
  contract `authnz`'s own reverse proxy (`authnz::proxy`) already
  implements for whatever it fronts (strip-then-overwrite of those
  three headers). **Catalog must not be reachable except through that
  gateway**; anything that can reach it directly can forge identity.
- Publishes `version.published` / `version.yanked` / `version.unyanked`
  to NATS (`src/events.rs`) via `typed_eventbus::Event` directly
  (`AppState::publish` in `src/state.rs`) rather than through
  `actixutils::locals::Context` / `actixutils::middleware::ReadContext<T>`
  the way `authnz`'s passwdless flow does — see that method's doc
  comment for why: `ReadContext` requires a `T: GetId` already in the
  request's extensions, i.e. the whole wrapped resource must be
  authenticated, and `/projects/{id}/versions` serves both an
  authenticated `POST` (upload) and an unauthenticated `GET` (list) at
  the identical path. Publishing straight through `EventStream` puts
  the same event on the wire (same producer/trace-id/user-id metadata
  `Context::publish` attaches) without that constraint.
- Has its own `catalog_db` (Postgres) per DES-006, with no live FK to
  Identity's `users` table — see the note at the top of the migration
  for why (cross-database FKs aren't a thing in Postgres, and DES-006
  says database-per-service).
- Answers Execution's `GetVersion` yank-status check (SDD §3.2/§6.3) as
  a plain REST `GET .../versions/{id}` rather than the `tonic` gRPC
  service in SDD §6.3 — see "Open items."

## Endpoints (SDD §6.1 base path `/api/v1`)

| Method | Path | Requirement(s) |
|---|---|---|
| POST | `/projects` | PRJ-001, PRJ-016 |
| GET | `/projects/{id}`, `/projects/by-slug/{slug}` | DSC-002 |
| PATCH | `/projects/{id}` | PRJ-017, PRJ-018 |
| DELETE | `/projects/{id}` | PRJ-005 |
| PUT | `/projects/{id}/current-version` | PRJ-006 |
| POST | `/projects/{id}/versions` (multipart: `version_tag`, `artifact`) | PRJ-001, PRJ-002, PRJ-010–015 |
| GET | `/projects/{id}/versions`, `/projects/{id}/versions/{vid}` | PRJ-009 |
| POST | `/projects/{id}/versions/{vid}/yank` | PRJ-019 |
| POST | `/projects/{id}/versions/{vid}/unyank` | PRJ-022 |
| GET | `/discovery?q=&tags=&categories=&author_id=&sort=&page=&page_size=` | DSC-001, 003–009 |

## Validation pipeline (PRJ-002, PRJ-010–015)

`src/validation/`, structured like the `ValidationPipeline` /
`ValidationStage` skeleton in SDD §5.2 (adapted to be `async` and to
operate on an extracted `Workspace` — see the module doc comment
there for the two adaptations and why).

Two stages (`compilation`, `security_scan`) need a Rust toolchain /
`cargo-audit` / crates.io egress the box running this service might
not have (most CI/dev sandboxes don't). They're gated behind the
`real_compile_checks` Cargo feature, **off by default**:

- **Off** (default): every stage still runs, but compilation and
  security-scan report a non-blocking "skipped" pass with an
  explanation. PRJ-010/PRJ-013 are *not* enforced in this mode — fine
  for local development, not for a real deployment.
- **On** (`cargo build --features real_compile_checks`): shells out to
  `cargo check` and (if `cargo-audit` is on `PATH`) `cargo audit`
  against the extracted upload, with a timeout.

`size_limit` and `dependency_check` use the SDD's 100MB / a
500-dependency default respectively — both are explicitly open items
in the SDD itself (Appendix B, RS §15-2), overridable via
`CATALOG_MAX_ARTIFACT_BYTES` / `CATALOG_MAX_DEPENDENCIES`.

A failing upload is rejected outright (PRJ-014) — no `versions` row is
created — with the full per-stage report returned in the response body
(PRJ-015), so the `pending`/`validating` states in the migration's
`validation_status` check constraint exist for schema headroom (e.g. if
validation is later moved off the request path onto a queue) but
aren't reachable through the code as written; every version currently
in the table got there by passing synchronously.

## Deviations from the literal SDD text

- **`owner_id`/`yanked_by`/etc. aren't FKs to `users`** — see above;
  DES-006 already implies this, the SDD's SQL just didn't say so
  explicitly for the catalog schema.
- **Categories** (PRJ-016, DSC-004) got a `project_categories` table;
  the SDD's §7.2 schema only modeled tags. Added as a separate
  many-to-many table alongside `project_tags` rather than folding into
  it, since "filter by category" (a small controlled list) and "filter
  by tag" (free-form) are asked for as independent facets in DSC-004.
- **No `tonic`/protobuf `Catalog` service** (SDD §6.3) — Execution's
  `GetVersion` yank-check is a REST `GET` instead; see "Open items."
- **Popularity score is `DOUBLE PRECISION`**, not `NUMERIC`/`DECIMAL`
  as in the SDD's billing-adjacent schemas — it's an accumulating
  counter fed by async events, not money, so exact decimal arithmetic
  isn't needed and it avoids a `rust_decimal` dependency for one field.
- **Object storage is a local-filesystem `ArtifactStore` impl**
  (`src/artifact_store.rs`), not S3 — the trait is the real interface;
  swapping in an S3-backed implementation for production is additive.

## Open items (carried from / added to SDD Appendix B)

- **RS §15-2** (max size / dependency limits) — inherited from the SDD,
  still unresolved; see `PipelineConfig` defaults above.
- **gRPC `Catalog`/`Execution` services** — SDD §6.3 specifies `tonic`;
  this deliverable implements the equivalent REST endpoint only
  (`GET /projects/{id}/versions/{vid}` already returns
  `eligible_for_new_deployment`, which is what `GetVersion`'s caller
  actually needs). Wiring an actual `tonic` service around the same
  `VersionRepo::get` call is additive, not a redesign, when Execution
  needs the real protocol.
- **`typed_eventbus::nats::connect(...)` in `main.rs::build_event_stream`**
  — this deliverable had `actixutils`'s source (which only holds an
  `Arc<dyn EventStream>`, never constructs one) but not
  `typed-eventbus`'s own source, so the exact connect/construction call
  is a best-effort placeholder, clearly marked in `main.rs`. If it
  doesn't compile against your actual `typed-eventbus` version, sharing
  that crate's source — the way `actixutils.zip` was shared to fix the
  `Context`/`ReadContext` wiring — will let this be corrected precisely.
- **`real_compile_checks` in production** — decide the deployment story
  (dedicated build-toolchain sidecar? Same container as the API?)
  before relying on PRJ-010/013 enforcement; see "Validation pipeline."
- **Compliance export/deletion** (SDD §8.3/§6.4): when Admin & Audit's
  `DataDeletionHandler` needs to purge or pseudonymize a user's
  projects, Catalog doesn't yet expose anything for that coordination
  — not built here since it's driven by Admin's endpoint, which is out
  of this service's scope, but it'll need either an internal endpoint
  or a subscribed event (e.g. `compliance.deletion_requested`) added
  to `src/events.rs`'s counterpart (a subscriber, not a publisher) when
  Admin & Audit is built.
