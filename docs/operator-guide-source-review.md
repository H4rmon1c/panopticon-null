# Operator guide: source review

Live retrieval requires a persistent, expiring human review of a source's robots and terms. The ephemeral `--robots-reviewed` flag is deprecated and is no longer the primary authorization.

## Why it exists

The project only retrieves from a source after an operator has examined the source's robots and terms and recorded an immutable review artifact that captures what was reviewed and for how long it is valid. This replaces a one-time flag with a durable, verifiable gate.

## Workflow

### 1. Capture a robots snapshot

```console
pnull source review capture <source_id>
```

This retrieves `https://<reviewed-host>/robots.txt` through the DNS-safe HTTP layer, computes its SHA-256, and preserves the response bytes and HTTP receipt locally. It reports the status, digest, observations, and byte count. A 404 response (including an empty body) is preserved as an explicit *absent robots file* receipt; it is not treated as an automatic permission to fetch the source. Other 4xx and 5xx responses fail capture. Recording the human review is a separate step.

### 2. Record the human review

```console
pnull source review record <source_id> --reviewer <name> --note <text> --expires <date>
```

This refuses to record a review until a `source review capture` within the last
24 hours has preserved `robots.txt` bytes and a matching HTTP fetch receipt. It
binds the latest capture made after the previous review to an immutable artifact
containing:

- source ID and source-config digest;
- reviewed hosts;
- reviewed endpoint patterns;
- robots URL, verified snapshot digest, and fetch receipt ID;
- terms URLs and digests when supported (the current configured sources do not
  provide terms URLs or capture terms snapshots; document the terms you checked
  in the reviewer note);
- reviewer, note, and review timestamp;
- expiration timestamp;
- minimum request interval;
- restrictions;
- the superseded review (none on first record).

When the robots response is 404, the reviewer note must explicitly mention
`404` and explain what policy and terms were checked. This follows the
[robots.txt standard](https://www.rfc-editor.org/rfc/rfc9309.html#section-2.3.1):
an unavailable 4xx robots file does not by itself grant permission from the
source owner, and unreachable 5xx/network failures are not treated as 404.

For a preserved 2xx robots response, applicable `Disallow` rules for `*`,
`pnull`, or the collector's `PanopticonNull` user agent block review recording
and live verification even when a reviewer supplies a note. The check uses the
preserved response bytes and each reviewed endpoint path. It conservatively
denies ambiguous wildcard/query prefixes and is not a full RFC 9309 evaluator;
an operator should seek an official alternate source or permission rather than
trying to override a disallow rule in a note.

An old review with an empty robots digest or missing receipt is invalid for live
retrieval. Capture and record a new review; historical rows stay intact. If a
review already exists for the same source and timestamp, the insert is a duplicate
and is not applied.

### 3. Show the current review

```console
pnull source review show <source_id>
```

Displays the current review: reviewer, note, review/expiration timestamps, config digest, hosts, endpoints, minimum interval, and restrictions.

### 4. Verify before live retrieval

```console
pnull source review verify <source_id>
```

Checks that the review is present, current, in scope for the configured endpoint,
and still bound to the exact preserved robots bytes and HTTP receipt. On success
it prints `source <source_id> is reviewable and in scope`.

## What live retrieval refuses

Live retrieval (for example `pnull ingest` against a live source) refuses when any of the following holds:

- no source review exists for the source;
- the review lacks a matching preserved robots snapshot or fetch receipt;
- a 404 robots response was not explicitly acknowledged in the review note;
- a preserved 2xx robots response disallows the reviewed endpoint;
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
