# PANOPTICON.FAIL public API contract

Status: frontend scaffold contract, not yet implemented by Panopticon Null.

Base path: `/api/v1`

This document defines the public, read-only boundary consumed by the PANOPTICON.FAIL Civic Intelligence Atlas. The API is not an administrative interface and must never query Panopticon's private working database on behalf of a public request. Every response comes from a sanitized publication artifact created only after review and publication gates succeed.

## Product model

The same publication-safe records support two presentation modes:

- **Public:** place-first questions, plain-language context, documented connections, and explicit unknowns.
- **Reporter:** advanced queries, source-first/change-first reading, evidence export, citation export, and an optional wide-area relationship lens.

Presentation mode never changes which claims are public.

## Publication invariants

1. Collection does not imply publication.
2. Every public claim has a current approved review binding.
3. Every public claim points to a source and exact evidence locator.
4. No pending, rejected, superseded, stale, or mismatched review is exposed.
5. No collector credentials, operator notes, internal paths, reconciliation candidates, private source configuration, or unpublished evidence appear in responses.
6. Public IDs are stable and safe for URLs.
7. A failed publish leaves the previous public dataset untouched.
8. The API process has read-only access to the public read model.
9. Relationships are source-backed records, not frontend inference.
10. People appear only in documented public or organizational roles; private-person surveillance is outside this API.
11. Unknowns remain explicit and must not be converted into allegations.

A future publisher should build `public.sqlite.tmp`, validate it, fsync it, then atomically replace `public.sqlite`. The API should reopen or reload only after a complete successful replacement.

## Core objects

The read model should remain domain-generic:

```text
ENTITY
RELATIONSHIP
EVENT
RECORD
SOURCE
CLAIM
CITATION
LOCATION
PROJECT
CONTRACT
DECISION
```

Useful entity types include:

```text
ORGANIZATION
AGENCY
COMPANY
PERSON
OFFICIAL
FACILITY
DATACENTER
CAMPUS
VENDOR
CONTRACTOR
UTILITY
NETWORK
HARDWARE
PROGRAM
JURISDICTION
PERMIT
VOTE
ORDINANCE
```

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

## Required endpoints

```text
GET /status
GET /activity?limit=12
GET /entities?limit=100&sort=updated
GET /search?q=<query>&limit=12
GET /entities/<id>
GET /evidence/<id>
GET /sources/<id>
```

Collections may return direct arrays or `{ "items": [...] }` envelopes.

## Recommended civic endpoints

The first atlas can derive local views from entity records and search. A production place index should eventually expose:

```text
GET /places/search?q=<town-county-or-zip>&limit=10
GET /places/<id>/summary
GET /places/<id>/activity?limit=50
GET /places/<id>/graph?depth=1
GET /places/<id>/unknowns
GET /changes?place=<id>&since=<timestamp>
```

The browser should not request device geolocation. A place is selected through deliberate user input or a shareable place identifier.

## `GET /status`

```json
{
  "state": "ok",
  "schema_version": "pnull-public-v1",
  "dataset_version": "2026-09-02T20:00:00Z",
  "last_ingest": "2026-09-02T19:57:00Z",
  "last_publish": "2026-09-02T20:00:00Z",
  "manifest_digest": "sha256:...",
  "counts": {
    "entities": 12418,
    "records": 46210,
    "sources": 193,
    "relationships": 28174
  }
}
```

`last_ingest` may describe the collector's last completed acquisition cycle, but only as a coarse timestamp deliberately copied into the public read model. It must not expose active jobs or internal source state. The frontend labels stale publications rather than presenting them as live.

## `GET /activity?limit=12`

```json
{
  "items": [
    {
      "id": "activity_...",
      "type": "SOURCE_CHANGED",
      "summary": "Official procurement record changed",
      "timestamp": "2026-09-02T19:31:08Z",
      "entity_id": "entity_...",
      "status": "PUBLISHED"
    }
  ]
}
```

Activity is publication activity, not a raw collector log. It must not reveal records that failed or are waiting for review.

## `GET /entities?limit=100&sort=updated`

```json
{
  "items": [
    {
      "kind": "entity",
      "id": "entity_...",
      "type": "ORGANIZATION",
      "name": "Example Organization",
      "subtitle": "Documented infrastructure operator",
      "updated_at": "2026-09-02T19:20:12Z",
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

Results may mix entities and sources.

## `GET /entities/<id>`

An entity response must contain enough data for a plain-language record, atlas node, timeline, and source inspection:

```json
{
  "id": "entity_...",
  "type": "FACILITY",
  "name": "Example Project",
  "subtitle": "Publicly documented infrastructure project",
  "status": "ACTIVE",
  "description": "A strictly non-inferential public summary.",
  "geo": {
    "lat": 38.8339,
    "lon": -104.8214,
    "label": "Colorado Springs, Colorado"
  },
  "jurisdiction_ids": ["jur_example_city", "jur_example_county"],
  "aliases": ["Example Project One"],
  "tags": ["INFRASTRUCTURE", "PROJECT"],
  "updated_at": "2026-09-02T19:35:44Z",
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

`confidence` is not a probability that a fact is true. For the first implementation it should represent a deterministic relationship state with documented semantics, or be omitted. The percentage visualization is provisional and should be revisited before production.

## `GET /evidence/<id>`

Every public fact, relationship, and timeline event should be evidence-addressable.

```json
{
  "id": "evidence_...",
  "source_id": "source_...",
  "claim": "The public claim text.",
  "quote": "The exact approved source excerpt.",
  "locator": "Page 14 · large-load allocations",
  "page": 14,
  "authority": "OFFICIAL UTILITY RECORD",
  "retrieved_at": "2026-09-02T19:30:12Z",
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
  "retrieved_at": "2026-09-02T19:30:12Z",
  "document_date": "2026-08-31",
  "sha256": "...",
  "description": "A public description of the source."
}
```

`canonical_url` must be an approved public `https://` URL or an internal canonical evidence route deliberately published by Panopticon. The frontend independently refuses non-HTTP(S) schemes.

## Explicit unknowns

The UI currently derives unknowns conservatively from absent public fields. The preferred production model is for the publisher to emit reviewable unknown-state records:

```json
{
  "subject_id": "entity_...",
  "field": "public_cost",
  "state": "NOT_ESTABLISHED",
  "as_of": "2026-09-02T20:00:00Z",
  "searched_source_classes": ["CONTRACT", "AGENDA", "BUDGET"],
  "note": "No publication-safe public amount is linked in the current dataset."
}
```

`NOT_ESTABLISHED` means the current public dataset does not establish the fact. It does not prove that the fact does not exist.

## Suggested process boundary

```text
pnull-collector user
  read/write: private state, immutable evidence, temporary publication output
  write: completed sanitized publication artifact
  network: outbound acquisition only; no public listener

pnull-api user
  read-only: current sanitized public artifact
  listen: 127.0.0.1 or Unix socket only
  no access: private state, unpublished evidence, credentials

caddy user
  read-only: static civic-atlas assets
  proxy: /api/* to pnull-api
```

The API should send a dataset ETag derived from `manifest_digest`. Entity, source, and evidence responses may use immutable caching when their URL includes a content-stable ID; search, activity, place summaries, and status should use short caching.
