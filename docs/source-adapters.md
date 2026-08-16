# Source adapters

## Selected jurisdiction

Version 0.0.1 monitors **Colorado Springs, Colorado**. The City directs users seeking recent agendas and minutes to its Legistar portal:

- City discovery: <https://coloradosprings.gov/city-council-meetings>
- City document guidance: <https://coloradosprings.gov/citydocs>
- Public calendar: <https://coloradosprings.legistar.com/Calendar.aspx>
- Documented API: <https://webapi.legistar.com/Help/Api/GET-v1-Client-Events>
- Configured collection: the URL in `configs/states/co.toml`, using documented OData filtering, ordering, a bounded top count, and expanded event items.

Colorado Springs was selected because the City officially links a stable, ID-addressable, structured public meeting system and because official matter 25-581 includes concrete surveillance-system references. The official City solicitation index is informational and points to BidNet as authoritative; BidNet may require registration and has restrictive terms, so v0.0.1 does not automate it.

## Adapter behavior

The JSON adapter accepts a Legistar event collection, one expanded event object, or a matter-text object. It extracts event date, agenda status, matter file/title, action, vote text, and statically stripped minutes RTF. Missing expanded `EventItems` is a structured extraction failure.

Live retrieval requires `--robots-reviewed`, permits one same-host HTTPS redirect chain of at most five responses, limits bytes and time, and persists a 24-hour source interval. The current robots body could not be independently verified through the research browser on 2026-08-16. The documented API was manually retrieved at a conservative one-request-at-a-time rate. No API-specific quota or affirmative bulk-use terms were found. Production operators must re-check before retrieval.

## Preserved demonstration matter

File 25-581 / Matter API ID 12913 concerns a Police Department Technology Surcharge. Fixtures preserve:

- draft ordinance attachment `14876734`;
- supporting presentation `14876735`;
- signed ordinance attachment `14995655`;
- work-session event `2654`;
- final-vote event `2660`.

Exact URLs and hashes are in `fixtures/README.md` and `fixtures/co/SHA256SUMS`.

## Limitations

The API is a meeting source, not a complete procurement ledger. Some contracts and amendments may appear only in attachments, the City document system, BidNet, or a Colorado Open Records Act response. Absence from this feed proves nothing. v0.0.1 does not paginate beyond the configured bounded query, schedule itself, or monitor BidNet.
