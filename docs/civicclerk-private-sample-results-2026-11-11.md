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

## Paginated list sample, November 11, 2026

A subsequent private pagination sample captured two list pages each for Monument, Pueblo, Glenwood Springs, Firestone, Greeley, Pagosa Springs, Parker, Snowmass Village, Greenwood Village, Berthoud, and La Junta. Each page returned HTTP 200 and 15 event metadata entries, for 330 entries across those eleven tenants; every second page indicated additional results. For each of these tenants, all 15 first-page event IDs also appear in the earlier first-page samples above, so those 165 entries must not be counted again as newly discovered events. Timestamp ties were flagged for Monument, Firestone, Pagosa Springs, Parker, Snowmass Village, and Berthoud, so pagination boundaries may be ambiguous.

The separate Erie pagination trial captured four pages containing 15, 15, 15, and 6 entries. Its first page also overlaps the earlier sample on all 15 event IDs. Its final response said there were no further pages, but the adapter records `source_end_unverified`; timestamp ties were observed and neither that response nor this sample proves complete historical coverage. The lists may change between requests. These are metadata references, not decisions, individual votes, or screened documents. This pagination pass fetched no PDFs and performed no text or OCR screening. No results were promoted to the public dataset or research database.

Private report locations below are relative to `/home/owo/Documents/ChatGPT/Stonk/`. Each archive also retains the corresponding source and network receipts. Keep these directories outside served or public deployment paths.

- Erie: `.panopticon-private-civicclerk-recon-20261111/pagination-live-01/report/results.json` and `pagination-live-02/report/results.json` under the same parent.
- Firestone and Greeley: `.panopticon-private-civicclerk-recon-20261111/pagination-sample-firestone/report/results.json` and `pagination-sample-greeley/report/results.json` under the same parent.
- Monument, Pagosa Springs, and Parker: `.panopticon-private-civicclerk-archived-batch-02/pagination-sample-01/report/results.json`, `pagination-sample-pagosa/report/results.json`, and `pagination-sample-parker/report/results.json` under the same parent.
- Pueblo and Snowmass Village: `.panopticon-private-civicclerk-archived-batch-03/pagination-sample-01/report/results.json` and `pagination-sample-snowmass/report/results.json` under the same parent.
- Berthoud, Glenwood Springs, and La Junta: `.panopticon-private-civicclerk-archived-batch-20261111/pagination-sample-berthoud/report/results.json`, `pagination-sample-01/report/results.json`, and `pagination-sample-lajunta/report/results.json` under the same parent.
- Greenwood Village: `.panopticon-private-civicclerk-greenwood-20261111/pagination-sample-greenwood/report/results.json`.

## Hardened native-text replay, November 11, 2026

The later hardened, source-bound scanner replayed the **same twelve previously captured sample PDFs** described above, one at a time. It did not fetch additional PDFs, so these are not twelve new documents or additional jurisdiction coverage. Each run replayed the first-party portal anchor, list, selected file and preserved receipt chain. Ten returned `unverified_no_keyword_in_extracted_text`; Erie returned the already reported preliminary keyword lead; and Monument returned `incomplete` because the native extractor did not establish usable text. The scanner did not OCR any of these PDFs; the separate previously reported four-page Monument OCR remains a distinct earlier operation.

Native extraction can omit scanned or image-only content. No keyword in extracted text does not establish that a document or jurisdiction lacks relevant information. Erie’s lead is the same public-comment mention noted above, not an established approval or item vote. These are private research results, not public cases; no research database or public dataset was changed.

Private result paths below are relative to `/home/owo/Documents/ChatGPT/Stonk/`. Each directory contains `results.json`; the retained source and capture archives named earlier in this note hold the original bytes and receipts. Keep all of them outside served paths and public artifacts.

- Monument: `.panopticon-private-civicclerk-archived-batch-02/private-pdf-screen-us-co-place-0851800-20261111/`
- Pagosa Springs: `.panopticon-private-civicclerk-archived-batch-02/private-pdf-screen-pagosa-20261111/`
- Parker: `.panopticon-private-civicclerk-archived-batch-02/private-pdf-screen-us-co-place-0857630-20261111/`
- Pueblo: `.panopticon-private-civicclerk-archived-batch-03/private-pdf-screen-us-co-place-0862000-20261111/`
- Snowmass Village: `.panopticon-private-civicclerk-archived-batch-03/private-pdf-screen-us-co-place-0871755-20261111/`
- Berthoud: `.panopticon-private-civicclerk-archived-batch-20261111/private-pdf-screen-us-co-place-0806255-20261111/`
- Glenwood Springs: `.panopticon-private-civicclerk-archived-batch-20261111/private-pdf-screen-us-co-place-0830780-20261111/`
- La Junta: `.panopticon-private-civicclerk-archived-batch-20261111/private-pdf-screen-us-co-place-0842110-20261111/`
- Greenwood Village: `.panopticon-private-civicclerk-greenwood-20261111/private-pdf-screen-us-co-place-0833035-20261111/`
- Erie: `.panopticon-private-civicclerk-recon-20261111/private-pdf-screen-us-co-place-0824950-20261111/`
- Firestone: `.panopticon-private-civicclerk-recon-20261111/private-pdf-screen-us-co-place-0826600-20261111/`
- Greeley: `.panopticon-private-civicclerk-recon-20261111/private-pdf-screen-us-co-place-0832155-20261111/`
