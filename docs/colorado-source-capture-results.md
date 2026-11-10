# Colorado four-case source capture (2026-11-10)

The reviewer **h4rmon1c** attested to the exact 12 release-cited URLs in the
[source-policy packet](colorado-source-review-pending.md). The
[batch gate](colorado-source-review-batch.md) recorded 12 endpoint-level
reviews, expiring **2026-12-09 22:30:23 UTC**. A second run found 12 matching
existing reviews and wrote none. It recorded zero reviews for the five
attachment URLs whose hosts returned `Disallow: /`.

Each approved endpoint then received one bounded `pnull source archive`
attempt. All 12 returned HTTP 200 and the preserved raw blobs rehashed to
their recorded SHA-256. A separate read-only check reopened the private SQLite
fetch receipt and content-addressed blob for every endpoint and verified the
exact URL, status, digest, and byte count. All 12 source reviews also pass
`pnull source review verify`. Eleven raw responses exactly match the committed
fixtures and source registry. The only mismatch is
`castle-rock-res2025-071-detail`:

| Source | Committed fixture SHA-256 | Live raw SHA-256 | Difference |
| --- | --- | --- | --- |
| Castle Rock RES 2025-071 detail page | `7ab7e31f16a7355cf45b15c881ca1d4680bec50f249b5d579ca7e6c5fb9f16f1` | `9b03f0ac93d31b109bd311c8afe5907ec18759e43cd6e3a45edaa0e10c608604` | Four HTML lines: CSS and image cache timestamps, ASP.NET `__VIEWSTATE`, and `__EVENTVALIDATION`. |

Both Castle Rock responses are 125,409 bytes. Masking only those four dynamic
fields makes their HTML byte-identical. The live raw response remains preserved
under its own digest; the fixture and registry were **not** silently changed.
By case, all three cited sources for each Commerce City ordinance and all four
for Denver 25-0413 match exactly. Castle Rock has three exact matches and this
one raw-byte mismatch. This is source-capture status, not a public case release.
The other eleven exact response digests are in the
[machine-readable packet](colorado-source-review-packet.json) and the private
attempt report at `<private-data-dir>/colorado-source-archive-attempts.json`.

This capture establishes provenance for the 12 fetched endpoints. It does not
establish a signed contract where the case manifests mark that link
`not_established`, authorize fetching the five blocked attachments, or make a
private preview public. A production export against this private capture store
still refuses because the global `atlas_release` allowlist is absent; source
capture alone never supplies it. The Castle Rock live detail receipt does not match the
fixture digest in its registry row; any release path demanding an exact digest
still needs a reviewed revision or a verified alternative source. Raw response
changes must remain visible in correction history.
