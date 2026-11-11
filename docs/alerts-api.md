# Public alerts and news API v1

The self-hosted release builds a read-only API from the same validated public snapshot used by the atlas. The API is static JSON served by Caddy. It does not query the collector database, accept writes, or call a government API when a reader polls it. These URLs become available after the VPS and DNS deployment:

| Endpoint | Purpose |
| --- | --- |
| `/api/v1/alerts.json` | Current released decisions and retained withdrawal notices, with per-case revision identifiers and exact evidence locators. |
| `/api/v1/feed.json` | JSON Feed 1.1 representation for news readers; content is plain text. |
| `/api/v1/coverage.json` | Published case counts by Colorado geography entry, with separate publication and search-coverage states. |
| `/api/v1/research.json` | Preliminary source references and machine-matched recorded actions and votes, with capture digests and locators. |
| `/api/v1/health.json` | Independently updated collection, publication, and source-review health. Failed publication can update health while the previous release remains available. |

`alerts.json` uses schema `pnull-alerts-v1`. Each record has a stable `case_id`, its canonical `url`, `state` (`active` or `withdrawn`), a positive `revision`, and a `revision_id`. The content digest binds the public summary, evidence references, source digests, gaps, and release binding. A repeat export of identical data retains the revision ID and change time. A changed summary or evidence binding advances the revision; a later return to previous content still receives a new revision. Consumers should persist each case's latest revision ID, rather than treating every poll or build as new news.

The coverage endpoint's `publication_state` describes whether a geography entry has released decisions. Its older `coverage_state` field remains a compatibility alias for publication state. The separate `search_coverage` object describes what the public inputs establish about jurisdiction searches. The current snapshot and geography roster contain no verified search ledger, so `state` is `not_established` and `last_searched_at`, `lookback_period`, `source_count`, `document_count`, and `receipt_count` are null, including for places with published cases. The total `jurisdictions_with_established_search_coverage` is also null. These are unknown values, not zero historical searches or findings of no relevant contracts. The directory size and number of places without published decisions must never be reported as jurisdictions searched.

The `change` field identifies the latest transition: `published`, `updated`, `withdrawn`, or `reinstated`. It does not mean that the transition happened during the reader's most recent poll. The API contains current state and the latest change per case, not a complete event history. A consumer that needs a historical audit trail must retain its own received versions. There is no pagination or server-side query API in v1; filter the bounded dataset locally.

`decision_date` is the date recorded on the decision. `first_published_at` and `changed_at` are API publication times. `generated_at` is the time this API artifact was built. **None of those timestamps proves that an official source was fetched or verified again.** Every record deliberately reports `source_last_checked_at: null`; raw-source acquisition freshness must be assessed from the independent health endpoint and collector records. A successful repeat publication cannot make a failed or stale collector healthy.

The output preserves recorded outcomes such as `Fail`, distinguishes direct votes from consent-block motion context, and includes explicit unknowns. Source references contain HTTPS URLs, preserved SHA-256 digests, exact locators, and verification digests. A summary does not establish execution, payment, or deployment unless the underlying released case establishes it. Private reviewer notes, private paths, raw collector data, and arbitrary added snapshot fields are excluded by an explicit output field list.

## Corrections and withdrawals

Publication supplies the previous release's `alerts.json`. Removing a previously active case without an explicit withdrawal fails the entire staged release. Withdrawal replaces the prior claims with a generic notice and advances the revision. Later releases retain this tombstone and its noindex correction page even when it is absent from that release's snapshot; a reinstatement advances the revision again. There is no silent deletion or automatic recycling of an earlier revision ID. Retain the previous API artifact in backups because it holds this revision continuity.

Withdrawing an item from a future bot should cause a correction or retraction of the earlier post, not a new allegation. An updated record should be evaluated as a correction or change to the existing record. The feed is not a command to post. No social posting or X credentials are included in this API.

## Consumer behavior

1. Poll `/api/v1/health.json` and `/api/v1/alerts.json` conservatively, for example every five minutes, with a request timeout and a 32 MiB response limit. Use conditional requests when the server supplies an ETag, honor retry instructions, and back off on errors.
2. Require the documented schema and expected canonical origin. Treat all titles, summaries, and source text as untrusted plain text. Never execute content or render it as HTML.
3. Establish a baseline on first startup. Post historical records only through an explicitly chosen backfill workflow; the first poll alone does not make old decisions breaking news.
4. Compare each case's current `revision_id` with durable consumer state. Check collection and publication health, review expiry, and actual evidence freshness before deciding whether a revision is suitable for a new alert. Require a recent health artifact as well as a healthy state: with the hourly health timer, reject a health `generated_at` older than two hours even if it still says `ok`. On stale or failed health, pause automatic new-claim posting; continue handling withdrawals and corrections.
5. Preserve failed outcomes and unknown links when composing text, and link the canonical decision page. Do not infer an item-specific vote from a consent-block vote. Later social-bot work must implement idempotency, correction/retraction handling, rate limits, and its own posting authorization.

## Build and verification

The publisher invokes the generator on its private staged release before changing the public symlink:

```console
node web/panopticon-terminal/scripts/build-alert-api.mjs STAGE https://panopticon.fail PREVIOUS_ALERTS_JSON
node --test web/panopticon-terminal/scripts/test-alert-api.mjs
```

Omit the last argument only for the first API release or migration from a release that has no API. Supplying a missing, malformed, duplicate, future-dated, wrong-origin, or inconsistent previous API fails publication. Preview/demo snapshots, missing case release bindings, unsafe URLs, duplicate decision slugs, text control characters, silent active-case removal, oversized fields, symlink inputs, and existing output paths are rejected. Input reads are bounded to 32 MiB through an open file descriptor; output is limited to 32 MiB and 10,000 records, including tombstones. Exceeding a bound fails instead of truncating evidence or deleting old notices; introduce versioned shards before that limit is reached.

The regression suite checks unchanged exports, revisions and reversions, withdrawal retention, reinstatement, failed-motion wording, explicit gaps, private-field exclusion, malicious markup, preview/demo rejection, canonical URLs, tampered prior records, bounded reads, and symlink rejection. The API generator is removed from the public release after staging.
# Preliminary research references

`GET /api/v1/research.json` is a separate static, read-only discovery dataset.
Every entry is `unconfirmed_source_reference`: a public-source keyword match,
not a verified decision, contract execution, or deployment. Some matter entries
also include source-matched recorded actions and individual vote rows. Each
action has separate history, meeting-item, and votes URLs and digests; each
vote has its exact array locator. `vote_scope` distinguishes an item vote,
a separate consent-block vote, a consent-designated item whose precise vote
scope is uncertain, and a missing vote. Failed outcomes remain failed. Older
captures are labeled when the source history changes.
Records include a stable source-based ID, exact capture URL, digest, locator,
timestamp, and relevant terms. `lookback_period` is null; do not infer complete
jurisdiction coverage from an empty result. OCR-derived text is labeled. This
dataset is not automatically added to the verified alerts/feed and must not be
treated as an authorized posting queue. Raw documents and receipts are kept in
private research state and rechecked during export.
