# Colorado special-district discovery leads

The collector reads a pinned, minimal extract of Colorado DOLA's
[All Special Districts in Colorado](https://data.colorado.gov/Local-Aggregation/All-Special-Districts-in-Colorado/7rar-v5z8)
dataset. The [captured JSON](../web/panopticon-terminal/geography/source-archives/dola_websites.json)
contains only district IDs, names, source status codes, and website fields.
It was captured on November 11, 2026 from the exact URL in
[`colorado_special_districts.py`](../scripts/colorado_special_districts.py),
with SHA-256 `9976b7a50d23cbb0883eab140e11e4df3276c0882d61387760f492b2de6f8c6b`.
The same raw response and network receipt are retained in the private research
archive. The state describes its geographic layer as provisional; this
extract is a directory lead, not a legal-status or boundary determination.

The collector joins the state LGID and exact name to the existing provisional
geography, selects source status code `1.0` without claiming its legal meaning,
and uses only a unique, syntactically valid root domain. It excludes hosts shared
with another district, municipal or school directory website, and known county
website, as well as non-root paths and shared platforms. Each visited homepage
must show the complete district name in displayed text or image alt text. The
raw page and receipt must still verify on export. A source-linked legislative
portal is not automatically attributed to a special district. No vote, office,
contract, or responsible official is inferred from these website leads.

Discovered agenda and minutes documents are screened using the existing
bounded network, sandbox, evidence digest, and locator checks. A matching
document remains an unconfirmed source reference, with a generic title and no
personal-name or vote extraction. A failed fetch, rejected homepage identity,
shared domain, inaccessible document, or missing match is not evidence that a
district had no relevant decision. Non-root and shared sites need scoped tenant
adapters before automated document attribution. The schedule checks unseen
websites first and then the oldest attempt, within the existing per-run limits.

The source is pinned intentionally. Refresh requires a new captured response,
receipt, digest, identity-join review, tests, and a new code revision; a live
network response never silently replaces the pinned directory.
