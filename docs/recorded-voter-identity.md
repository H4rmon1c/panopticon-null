# Recorded-voter identity boundary

The atlas groups direct roll calls across decisions only when the archived
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

Consent-agenda block votes are excluded from this grouping as votes on an
individual item. A consent item can show the block motion as context while
leaving its item-specific voter list empty.
