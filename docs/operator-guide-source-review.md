# Operator guide: source review

Live retrieval requires a persistent, expiring human review of a source's robots and terms. The ephemeral `--robots-reviewed` flag is deprecated and is no longer the primary authorization.

## Why it exists

The project only retrieves from a source after an operator has examined the source's robots and terms and recorded an immutable review artifact that captures what was reviewed and for how long it is valid. This replaces a one-time flag with a durable, verifiable gate.

## Workflow

### 1. Capture a robots snapshot

```console
pnull source review capture <source_id>
```

This retrieves `https://<reviewed-host>/robots.txt` through the DNS-safe HTTP layer, computes its SHA-256, and preserves the snapshot locally as content-addressed evidence bytes. It reports the snapshot digest, the number of fetch observations, and the byte count. A snapshot is preserved locally; recording the human review is a separate step.

### 2. Record the human review

```console
pnull source review record <source_id> --reviewer <name> --note <text> --expires <date>
```

This records an immutable review artifact containing:

- source ID and source-config digest;
- reviewed hosts;
- reviewed endpoint patterns;
- robots URL, snapshot digest, and provenance;
- terms URLs;
- reviewer, note, and review timestamp;
- expiration timestamp;
- minimum request interval;
- restrictions;
- the superseded review (none on first record).

If a review already exists for the same source and timestamp, the insert is a duplicate and is not applied.

### 3. Show the current review

```console
pnull source review show <source_id>
```

Displays the current review: reviewer, note, review/expiration timestamps, config digest, hosts, endpoints, minimum interval, and restrictions.

### 4. Verify before live retrieval

```console
pnull source review verify <source_id>
```

Checks that the review is present, current, and in scope, and prints `source <source_id> is reviewable and in scope`.

## What live retrieval refuses

Live retrieval (for example `pnull ingest` against a live source) refuses when any of the following holds:

- no source review exists for the source;
- the review is expired;
- the source configuration digest changed since the review (source configuration changed);
- the reviewed hosts no longer include the source's reviewed host (allowed hosts changed);
- the requested endpoint is outside the reviewed scope;
- a prior response announced restrictions (the review's restrictions are non-empty), requiring renewed review.

## Expiration and renewal

Because a review has an expiration timestamp, an operator must periodically re-examine the source's robots and terms and record a new review before the old one expires. A prior restriction requires renewed review; live retrieval stays refused until then.

## Notes

- Reviews are append-only: a later review for the same source supersedes earlier ones without deleting history.
- Reviews are stored in SQLite as supplemental records; they do not alter canonical v0.0.1 evidence.
- Production operators must re-check robots and terms before any live retrieval; a recorded review is an operator attestation, not a legal guarantee.
