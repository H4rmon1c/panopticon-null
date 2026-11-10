# Colorado source-policy review packet (2026-11-10)

**Later status:** h4rmon1c approved the 12 release-cited endpoints on
2026-11-10. Their exact source reviews and one bounded live capture per
endpoint are recorded in the private store. See the
[capture result](colorado-source-capture-results.md). The five disallowed
attachments remain excluded. The pending language below describes the packet
at the time it was assembled.

This packet covers the 17 exact source URLs cited by the four Colorado v2 case
manifests before the robots audit. The [machine-readable packet](colorado-source-review-packet.json)
lists every registry source ID, archive source ID, exact endpoint URL, source
configuration SHA-256, preserved fixture SHA-256, robots response digest and
fetch receipt ID. All 17 committed fixture files match their registry digests;
that is an **offline integrity check**, not a claim that the live endpoint still
serves the same bytes.

The private state and raw robots responses are in the operator's private
source-capture data directory, outside this repository.
The 17 robots captures occurred on 2026-11-10. No source-policy review was
recorded, no case endpoint was fetched by this packet, and no case was published.
The preserved robots captures must be refreshed if more than 24 hours pass
before a human records a source review.

## Shared-host findings

| Host | Case endpoints | Robots response | Next action |
| --- | ---: | --- | --- |
| `webapi.legistar.com` | 9 | HTTP 404; body SHA-256 `dc1d54dab6ec8c00f70137927504e4f222c8395f10760b6beecfcfa94e08249f` | Human policy/terms review; 404 is not permission. |
| `commerce.legistar.com` | 1 | HTTP 404; same 404 body digest | Human policy/terms review. |
| `castlerock-co.legistar.com` | 1 | HTTP 404; same 404 body digest | Human policy/terms review. |
| `denver.legistar.com` | 1 | HTTP 404; same 404 body digest | Human policy/terms review. |
| `legistar.granicus.com` | 2 | HTTP 200; `User-agent: *` / `Disallow: /`; body SHA-256 `aa768c1c91a84875b1067e13ccad1a9ccbed13bffbbd223909cbb1c1dfaab585` | Do not fetch these attachments. |
| `legistar2.granicus.com` | 2 | HTTP 200; same `Disallow: /` body digest | Do not fetch these attachments. |
| `denver.legistar1.com` | 1 | HTTP 200; same `Disallow: /` body digest | Do not fetch this attachment. |

The [official Legistar API help](https://webapi.legistar.com/Help) documents
endpoints, but is not an explicit grant for bulk collection or republication.
[Granicus's Terms of Use](https://granicus.com/trust-center/terms-of-use/)
(dated 2026-07-20) describe individual/private use of material on “this
website.” Whether those corporate-site terms apply to municipality-operated
Legistar portals or their public records is **unresolved**. A bounded 200
response and headers for that page are preserved privately as
`.pnull/granicus-terms.html` and `.pnull/granicus-terms.headers`; the exact HTML
SHA-256 is `b2589afdcdceab570a201272e731a9c833ad838b6937fe68356a98d321c8e40d`
(132,002 bytes). The corporate `robots.txt` response is preserved in
`.pnull/granicus-robots.txt` with SHA-256
`ecc1d4dc9535f97ba906b700cc1c8e888559ab90c37a2a509a9b46f5e301998b`.
These corporate observations do not override the three attachment hosts'
own `Disallow: /` directives.

## Release-cited sources awaiting human endpoint review

The four release manifests now cite **12** sources: nine exact event, matter,
or vote JSON URLs and three city Legistar minutes/detail URLs. They are the
rows with `release_cited: true` in the packet. Their preserved robots responses
are 404, so retrieval still requires an actual human review of current site/API
terms and any relevant municipality policy. A reviewer note must expressly
acknowledge HTTP 404. The project records one expiring review **per fixed source
endpoint**, not one review per decision or vote row.

After the reviewer has examined all 12 exact URLs and current policies, one
explicit attestation can drive a bounded batch of the existing per-endpoint
`pnull source review record` calls. Such a batch should read only packet rows
where `release_cited` is true, compare the packet's config digest and robots
receipt with the current local state, require a nonempty reviewer name and
host-specific terms/404 note, and refuse any `blocked_by_robots` row. It must
never infer the reviewer from prior case approvals. No batch attestation has
been made or run here. The [operator guide](operator-guide-source-review.md)
has the individual command and 24-hour capture rule.

## Five blocked attachments and official alternatives checked

These five IDs remain in the source registry and offline fixtures for audit
history. They were removed only from the **public release manifests'**
`supporting_sources`. The fixture digests were not changed. Every published
decision, amount, vote, technology name, and agreement-status relationship in
those manifests still has a field-level proof from the 12 remaining sources;
the contract execution/payment links remain `not_established`. The v2 exporter
regression checks that no release case cites a blocked host.

| Registry source ID | What it is | Official alternative found | Remaining gap |
| --- | --- | --- | --- |
| `co-commerce-2026-ordinance` | Ord 2755 PDF attachment | [Commerce City legal notice](https://www.c3gov.com/News-articles/New-page-37) states the adopted title; the exact decision title is also in the official event record. | No verified same-byte city-hosted full ordinance copy. |
| `co-commerce-2026-unsigned-agreement` | Attached Flock agreement format | The event title establishes approval of an agreement **format**; signed execution is `not_established`. | No verified same-byte city-hosted agreement copy. |
| `co-castle-rock-2025-sole-source` | RES 2025-071 sole-source request | [Town Council file](https://castlerock-co.legistar.com/LegislationDetail.aspx?From=RSS&GUID=A667BD32-8DA2-4183-B796-C466D44BB3B3&ID=7420429) gives the decision title and staff memo. | No verified same-byte town-hosted request copy. |
| `co-castle-rock-2025-vendor-signed-agreement` | Master Services Agreement and Order Form attachment | The Council file establishes the authorized agreement; [the Town says Flock agreements are public records](https://www.crgov.com/1753/Automated-License-Plate-Reader), but that page may describe a different ALPR agreement. | No verified same-byte town-hosted DFR agreement copy or Town execution proof. |
| `co-denver-2025-flock-proposed-amendment` | Proposed amendatory agreement for failed 25-0413 | [Denver Council file](https://denver.legistar.com/LegislationDetail.aspx?GUID=A074C7B6-0FDA-447F-B62A-7B6B77C7BAD4&ID=7273047) records the proposal, dollar amounts, and failed outcome. | No verified same-byte city-hosted proposed amendment copy. |

The official Council pages expose `View.ashx` attachment links, but web access
to the Denver and Castle Rock candidate links returned HTTP 410. No claim of
same-byte equivalence was made. The [unsent public-records request drafts](colorado-records-request-drafts.md)
ask the three record custodians for direct copies or stable official URLs.

If a replacement is obtained, add it as a **new** registry source with its own
URL, raw-byte SHA-256, exact locator, archive configuration, and source-policy
review/receipt. Update the relevant manifest proof or supporting source only
after comparing the new document with the old offline fixture and checking its
claims. Do not reuse the old digest for different bytes or relax the receipt
gate. Until then, the release can state only the facts proven by the remaining
12 sources.
