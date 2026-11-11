# Colorado source review batch

This packet is the historical November 10 attestation. The subsequent Castle
Rock source-registry revision deliberately prevents replaying it against the
current repository. Restore the already recorded, unexpired source reviews
with the private release store; do not rewrite the original packet or its
attestation to authorize changed bytes. Offline tests replay the historical
context and separately verify that the revised registry is rejected.

`deploy/selfhost/record-source-review-batch.py` records one approved source-policy
decision across the 12 exact release-cited URLs in
`docs/colorado-source-review-packet.json`. It does not fetch case endpoints,
create cases, publish records, or approve the five blocked attachments.

The reviewer first inspects the packet, preserved robots responses, exact
endpoints, and applicable terms. A `404` robots response is not permission.
The reviewer writes a private attestation JSON file with:

- `schema_version`: `1`
- `packet_sha256`: SHA-256 of the **raw packet bytes**
- `reviewer`: the explicit reviewer name
- `expires_at`: a UTC RFC 3339 timestamp within 30 days
- `approved_source_ids`: the sorted list of all 12 release-cited registry IDs
- `host_notes`: one note for each of `webapi.legistar.com`,
  `commerce.legistar.com`, `castlerock-co.legistar.com`, and
  `denver.legistar.com`. Each note must acknowledge HTTP `404` and the
  uncertainty or restrictions in the applicable terms scope.

Keep the attestation in a private `0700` data directory. The helper checks the
packet against the current source registry, four case manifests, three fixed
archive configs, fixture bytes, and exact preserved robots receipts and blobs.
Every release-cited `404` must be less than 24 hours old. All checks finish
before the helper calls `pnull`; a bad packet writes no reviews. Repeating the
same attestation skips matching existing reviews and refuses conflicts.

```sh
python3 deploy/selfhost/record-source-review-batch.py \
  --data-dir /private/path/.pnull \
  --attestation /private/path/.pnull/colorado-source-review-attestation.json

python3 deploy/selfhost/record-source-review-batch.py \
  --data-dir /private/path/.pnull \
  --attestation /private/path/.pnull/colorado-source-review-attestation.json \
  --record
```

The first command is a read-only preflight. The second records the 12 reviews
through the existing Rust CLI, which independently enforces the robots and
review gates. `source archive` remains a separate bounded action for each
reviewed URL and is subject to its configured minimum interval. A failed
network fetch also consumes that interval; retry only after it expires.
