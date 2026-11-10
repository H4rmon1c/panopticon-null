# Consent agenda cases awaiting release treatment

These manifests are **private previews**. The exporter checks exact source bytes,
JSON locators, consent flags, motion fields, and the block roll-call ID. It
publishes **no individual vote on an item** when the recorded roll call belongs
to the consent block. The Rust production gate rejects all four pending cases.

| Case | Recorded action | Limit of the evidence |
| --- | --- | --- |
| Arapahoe County [24-633](arapahoe-county-24-633-2024-11-26.v2.json), 2024-11-26 | Flock 2025 service item approved on consent agenda | The four Aye rows belong to motion 38020; execution and payment are not established. |
| Arapahoe County [25-378](arapahoe-county-25-378-2025-07-22.v2.json), 2025-07-22 | Flock coverage expansion item approved on consent agenda | The five Aye rows belong to motion 40833. Staff estimate and draft order differ; neither is proof of payment. |
| Denver [25-1967](denver-25-1967-2025-12-08.v2.json), 2025-12-08 | Proposed Axon amendment adopted en bloc | The 13 Aye rows belong to block motion 199484. The title states a proposed $450,000 increase and $22,547,650.10 new total, not payment. City execution is not established by the attachment. |
| Denver [25-1968](denver-25-1968-2025-12-08.v2.json), 2025-12-08 | Proposed Axon framework adopted en bloc | The same block vote covers this and other items. The title states a proposed $27,000,000 agreement amount, not payment. City execution is not established by the attachment. |

The [Arapahoe final minutes](https://arapahoe.legistar1.com/arapahoe/meetings/2025/7/2604_M_Board_of_County_Commissioners_Business_Meeting_25-07-22_Meeting_Minute_Summaries_%28Business_Meeting%29.pdf) and [Denver final minutes](https://denver.legistar.com/View.ashx?GUID=01E28288-E47A-428D-88CB-D92504BF83C7&ID=1362533&M=M) are preserved with hashes under `fixtures/co-arapahoe/` and `fixtures/co-denver/`. The exact structured event and block-vote responses are preserved alongside them because the PDFs alone do not supply the same machine-checkable link between each item and the block motion. The source registry and archive profiles bind the preserved bytes to fixed official endpoints. No browser request to those endpoints is needed to show a preview.
