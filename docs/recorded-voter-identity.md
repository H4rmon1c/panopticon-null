# Recorded-voter identity boundary

The atlas groups official roll-call voter identities across decisions only when the archived
official vote rows supply the same `VotePersonId`, the same exact
`VotePersonName`, the same Legistar client, and the same verified jurisdiction.
The ID is scoped to that official portal; it is not a national person ID.
The source URLs, raw-byte digests, exact JSON row locators, and case-specific
vote evidence stay attached to each occurrence. A disagreement in name,
portal client, jurisdiction, or ID makes the bundle fail instead of merging
records by a similar name.

Commerce City Ordinances 2755 and 2756 each have a direct nine-person roll
call. The two decisions now link to nine shared recorded-voter entities, each
with two separate dated vote events. The roll-call sources establish the
recorded positions and portal IDs. They do **not** establish each person's
office, term dates, current office, or participation in a contract after the
vote. Those fields remain `not_established` until independent official role
and term evidence is archived and bound to exact claims.

Consent-agenda block rows can reuse the same scoped voter identity when the
portal ID and exact name match. They remain votes on the **block motion**,
never on an individual item. The item's own voter list stays empty, and a
block row adds no vote-on-item relationship or timeline event to the person's
record. The block source, row locator, digest, and motion ID are preserved.

Official headshots use this same scoped voter identity. See [the portrait
provenance method](official-portraits.md) for preserved image bytes, source
pages, and unresolved image matches.
