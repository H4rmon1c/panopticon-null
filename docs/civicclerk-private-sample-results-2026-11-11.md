# CivicClerk private sample results — November 11, 2026

These are bounded research samples, not statewide coverage or public decision records. The following paths are relative to the local workspace `/home/owo/Documents/ChatGPT/Stonk`; the archives are private and are not included in the repository.

Twelve source-linked tenants each returned the first page of 15 past events, for **180 event entries and 322 published-file references** in total. Each list reported `has_more: true`; no later list pages were fetched in this sample. One published PDF was privately captured from each list. A tenant link does not, by itself, establish the responsible body, a contract, a vote, or coverage of every record in that jurisdiction.

| Tenant | Events | File references |
| --- | ---: | ---: |
| Erie | 15 | 25 |
| Firestone | 15 | 23 |
| Greeley | 15 | 24 |
| Berthoud | 15 | 24 |
| Glenwood Springs | 15 | 33 |
| La Junta | 15 | 40 |
| Monument | 15 | 28 |
| Pagosa Springs | 15 | 27 |
| Parker | 15 | 32 |
| Pueblo | 15 | 28 |
| Snowmass Village | 15 | 24 |
| Greenwood Village | 15 | 14 |

The initial native-text screen of these twelve PDFs recorded one provisional rule match, ten `no_match` results, and one extraction error. The Erie match is a resident's public comment mentioning Flock cameras; it does not establish an approval or a vote. Monument's scanned PDF failed the initial native-only screen. A separate private page worker subsequently attempted OCR on all four pages of that PDF; the saved rule screen records no keyword matches on those four OCR results. OCR can miss or misread text, and a `no_match` or an attempted page is not evidence that a document or jurisdiction contains no relevant record. The native-text screen does not establish complete image coverage. No item-specific findings from these samples are automatically published.

The Monument source link and PDF identity were independently checked: the town council page links to the `monumentco` portal, event 557 lists minutes file 4894 for October 19, 2026, and page 1 of the preserved PDF identifies the same town council and meeting date. The PDF SHA-256 is `1aeb9bb48e7830c8e28af34f9d9ca19514b341144041948e160fb9ebc1fb7208`. That confirms the sample's document identity, not a surveillance-related decision.

Private local artifacts, relative to the workspace above:

- Erie, Firestone, Greeley: `.panopticon-private-civicclerk-recon-20261111/bounded-list-pass-08/report/results.json`; native screens: `.panopticon-private-civicclerk-recon-20261111/private-sampled-pdf-screens.json`.
- Greenwood Village: `.panopticon-private-civicclerk-greenwood-20261111/bounded-list-pass-01/report/results.json` and `private-sample-screen.json` in that pass directory.
- Berthoud, Glenwood Springs, La Junta: `.panopticon-private-civicclerk-archived-batch-20261111/bounded-list-pass-01/report/results.json` and `private-sample-screens.json` in that pass directory.
- Monument, Pagosa Springs, Parker: `.panopticon-private-civicclerk-archived-batch-02/bounded-list-pass-01/report/results.json` and `private-sample-screens.json` in that pass directory.
- Pueblo, Snowmass Village: `.panopticon-private-civicclerk-archived-batch-03/bounded-list-pass-01/report/results.json` and `private-sample-screens.json` in that pass directory.
- Monument page OCR and rule screen: `.panopticon-private-monument-ocr-20261111/monument/ocr-pages/` and `.panopticon-private-monument-ocr-20261111/private-rule-screen.json`.
