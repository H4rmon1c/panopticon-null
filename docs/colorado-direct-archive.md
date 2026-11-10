# Colorado direct-document archive

`pnull source archive <source-id>` privately preserves one fixed official HTML,
PDF, or JSON URL. It stores the exact response bytes by SHA-256 and the HTTP
fetch observations in the configured data directory. It does not parse the
document, make a claim, or publish to the atlas.

The Commerce City examples live in
`configs/states/co-commerce-archive.toml`. Its seven fixed URLs include
Ordinance 2755 and 2756 records, Council minutes, and the attached agreement.
The Castle Rock, Denver, and Arapahoe County profiles add 32 more fixed URLs.
The archive command does not crawl links or search for other records. JSON
event and vote URLs are included where the release pipeline needs a private
fetch receipt independent of its bundled fixture digest.

Before the first live fetch of each source, an operator captures its current
robots response and records a source-level review according to
[`operator-guide-source-review.md`](operator-guide-source-review.md). Review
expires, and changing the source URL or any other reviewed configuration
invalidates it. This is a review of the collection endpoint, not a review of
every record it returns. The collector refuses a missing, expired, or changed
review. The configured 24-hour interval is enforced per source, including
failed fetches.

Example, after a source review has been recorded:

```sh
pnull --config configs/states/co-commerce-archive.toml \
  --data-dir /var/lib/panopticon \
  source review verify commerce-ord2755-detail
pnull --config configs/states/co-commerce-archive.toml \
  --data-dir /var/lib/panopticon \
  source archive commerce-ord2755-detail
```

The printed digest locates the private blob under
`<data-dir>/evidence/sha256/<first-two-hex>/<digest>`. The fetch receipt is
stored in the same private SQLite state. Do not serve or commit that state.
The archive requires an exact final URL, a successful 200 response (or a 304
with preserved prior bytes), a matching response digest, expected content
type and signature, and the configured byte bound.

For deployment, add `archive` as an adapter in the collection matrix and
route it to `pnull source archive <source-id>` after
`pnull source review verify <source-id>`. The source-level review remains the
live fetch gate even if the scheduler is enabled.
