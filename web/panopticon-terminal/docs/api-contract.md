# Draft public API contract

Status: frontend scaffold contract, not yet implemented by Panopticon Null.

Base path: `/api/v1`

All endpoints are public and read-only. The API reads only a sanitized publication artifact produced after Panopticon's existing citation-review and publication-allowlist gates. It must not query the collector's operational database on behalf of public requests.

## Publication invariants

1. Collection does not imply publication.
2. Every public claim has a current approved review binding.
3. Every public claim points to a source and exact evidence locator.
4. No pending, rejected, superseded, stale, or mismatched review is exposed.
5. No collector credentials, operator notes, internal paths, reconciliation candidates, private source configuration, or unpublished evidence appear in responses.
6. Public IDs are stable and safe for URLs.
7. A failed publish leaves the previous public dataset untouched.
8. The API process has read-only access to the public read model.

A future publisher should build `public.sqlite.tmp`, validate it, fsync it, then atomically replace `public.sqlite`. The API should reopen or reload only after a complete successful replacement.

## Content type and errors

Success:

```http
Content-Type: application/json; charset=utf-8
Cache-Control: public, max-age=30, stale-while-revalidate=300
```

Error shape:

```json
{
  "error": {
    "code": "not_found",
    "message": "Public entity not found"
  }
}
```

Never return Rust debug output, SQL errors, filesystem paths, stack traces, or underlying database details.

## `GET /status`

```json
{
  "state": "ok",
  "schema_version": "pnull-public-v1",
  "dataset_version": "2026-09-01T05:41:22Z",
  "last_ingest": "2026-09-01T05:38:07Z",
  "last_publish": "2026-09-01T05:41:22Z",
  "manifest_digest": "sha256:...",
  "counts": {
    "entities": 12418,
    "records": 46210,
    "sources": 193,
    "relationships": 28174
  }
}
```

`last_ingest` may describe the collector's last completed acquisition cycle, but only as a coarse public timestamp deliberately copied into the public read model. It must not expose active jobs or internal source state.

## `GET /activity?limit=12`

May return an array directly or `{ "items": [...] }`.

```json
{
  "items": [
    {
      "id": "activity_...",
      "type": "SOURCE_CHANGED",
      "summary": "Official procurement record changed",
      "timestamp": "2026-09-01T05:31:08Z",
      "entity_id": "entity_...",
      "status": "PUBLISHED"
    }
  ]
}
```

Activity is publication activity, not a raw collector log. It must not reveal records that failed or are waiting for review.

## `GET /entities?limit=6&sort=updated`

```json
{
  "items": [
    {
      "kind": "entity",
      "id": "entity_...",
      "type": "ORGANIZATION",
      "name": "Example Organization",
      "subtitle": "Documented infrastructure operator",
      "updated_at": "2026-09-01T05:20:12Z",
      "source_count": 14
    }
  ]
}
```

## `GET /search?q=<query>&limit=12`

The first implementation may support plain text plus exact filters:

```text
type:facility colorado
type:contract power
source:official utility
```

Do not implement a free-form SQL-like language by concatenating user input into database queries. Parse filters into a closed enum and bind all values.

Results may mix entities and sources:

```json
{
  "items": [
    {
      "kind": "entity",
      "id": "entity_...",
      "type": "FACILITY",
      "name": "Example Campus",
      "subtitle": "Datacenter campus · Colorado",
      "updated_at": "2026-09-01T05:35:44Z",
      "source_count": 11
    },
    {
      "kind": "source",
      "id": "source_...",
      "type": "SOURCE",
      "name": "Official permit attachment",
      "subtitle": "OFFICIAL PERMIT RECORD · PDF",
      "updated_at": "2026-09-01T05:14:17Z",
      "source_count": 1
    }
  ]
}
```

## `GET /entities/<id>`

```json
{
  "id": "entity_...",
  "type": "FACILITY",
  "name": "Example Campus",
  "subtitle": "Datacenter campus · Colorado",
  "status": "ACTIVE",
  "description": "A strictly non-inferential public summary.",
  "aliases": ["Example Campus One"],
  "tags": ["DATACENTER", "CAMPUS"],
  "updated_at": "2026-09-01T05:35:44Z",
  "source_count": 11,
  "relation_count": 4,
  "attributes": [
    {
      "label": "POWER",
      "value": "96 MW documented planning capacity",
      "evidence_id": "evidence_..."
    }
  ],
  "relationships": [
    {
      "id": "relationship_...",
      "type": "POWERED_BY",
      "label": "powered by",
      "target_entity_id": "entity_utility_...",
      "confidence": 1.0,
      "source_count": 2,
      "evidence_id": "evidence_..."
    }
  ],
  "timeline": [
    {
      "id": "event_...",
      "date": "2026-08-28",
      "type": "CONSTRUCTION",
      "title": "Permit attachment records a project milestone",
      "evidence_id": "evidence_..."
    }
  ],
  "source_ids": ["source_..."]
}
```

`confidence` is not a probability that a fact is true. For the first implementation it should represent the deterministic relationship state using documented semantics, or be omitted entirely. The UI currently displays it as a percentage only to reserve layout space; this should be revisited before production.

## `GET /evidence/<id>`

```json
{
  "id": "evidence_...",
  "source_id": "source_...",
  "claim": "The public claim text.",
  "quote": "The exact approved source excerpt.",
  "locator": "Page 14 · large-load allocations",
  "page": 14,
  "authority": "OFFICIAL UTILITY RECORD",
  "retrieved_at": "2026-09-01T05:30:12Z",
  "sha256": "...",
  "review_state": "APPROVED",
  "review_bound_digest": "sha256:..."
}
```

Only fields approved for publication may appear. If exact quoted text is not approved for public display, the evidence record must not be published merely because the surrounding claim is approved.

## `GET /sources/<id>`

```json
{
  "id": "source_...",
  "title": "Official planning docket",
  "authority": "OFFICIAL UTILITY RECORD",
  "source_type": "PLANNING DOCKET",
  "canonical_url": "https://public.example/...",
  "retrieved_at": "2026-09-01T05:30:12Z",
  "document_date": "2026-08-31",
  "sha256": "...",
  "description": "A public description of the source."
}
```

`canonical_url` must be an approved public `https://` URL or an internal canonical evidence route deliberately published by Panopticon. The frontend independently refuses non-HTTP(S) schemes.

## Suggested process boundary

```text
pnull-collector user
  read/write: private state, immutable evidence, temporary publication output
  write: completed sanitized publication artifact
  network: outbound acquisition only; no public listener

pnull-api user
  read-only: current sanitized public artifact
  listen: 127.0.0.1 or Unix socket only
  no access: private state, evidence not selected for publication, credentials

caddy user
  read-only: static terminal assets
  proxy: /api/* to pnull-api
```

The API should send a dataset ETag derived from `manifest_digest`. Entity/source/evidence responses may use immutable caching when their URL includes a content-stable ID; search and status should use short caching.
