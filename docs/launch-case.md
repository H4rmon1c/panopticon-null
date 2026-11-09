# Colorado Springs launch case: Ordinance 25-93

**Editorial status:** source facts checked against preserved official records; h4rmon1c approved the case-level release on November 9, 2026. The production snapshot has been generated. Public access on `panopticon.fail` awaits domain verification. This is a decision about a Police Department Technology Surcharge. It is **not** a vote on an Axon or Flock contract.

## Short evidence card

**Colorado Springs City Council voted 6–3 on November 25, 2025, to amend city code to assess a police technology surcharge.** The signed ordinance directs surcharge proceeds to police technology systems. A staff presentation for the same matter listed Axon body cameras, digital evidence and AI transcription, and Flock vehicle-intelligence cameras among its 2026 major systems costs. The checked records do **not establish a named vendor contract, award, payment, or deployment caused by this vote.**

**Unresolved question:** Which specific contracts and payments, if any, will be funded from the surcharge proceeds?

Primary records: [Council meeting and action](https://webapi.legistar.com/v1/coloradosprings/events/2660?EventItems=1&AgendaNote=1&MinutesNote=1&EventItemAttachments=1) · [individual votes](https://webapi.legistar.com/v1/coloradosprings/EventItems/103550/Votes) · [signed ordinance](https://coloradosprings.legistar.com/View.ashx?M=F&ID=14995655&GUID=00173B3A-1CD9-4839-9AD5-93B3C8E4EE03) · [staff presentation](https://coloradosprings.legistar.com/View.ashx?M=F&ID=14876735&GUID=EA1466B1-D5D6-48CF-BB70-7710853E363D).

## Decision → contract → records

- **Jurisdiction:** City of Colorado Springs, Colorado.
- **Public body:** City Council. **Agency discussed:** Colorado Springs Police Department.
- **Decision:** Ordinance No. 25-93, matter 25-581, event item 103550; finally passed on November 25, 2025, by a recorded 6–3 vote. Nancy Henjum moved; Kimberly Gold seconded.
- **Recorded Aye votes:** Tom Bailey, Dave Donelson, Kimberly Gold, Nancy Henjum, David Leinweber, Roland Rainey Jr.
- **Recorded No votes:** Lynette Crow-Iverson, Brian Risley, Brandy Williams.
- **Individual office titles and terms:** **Not established** from the checked vote and meeting records. The names and recorded votes are established; a separate roster or election record is required to publish each person's office title and term dates.
- **Contract or award:** **Not established** by these records. The ordinance authorizes a surcharge and directs its proceeds; it does not identify an executed contract with either named vendor.
- **Technology link:** Supporting staff presentation, page 7, lists 2026 major systems costs for Axon body-worn cameras, Axon digital evidence and AI transcription, and Flock vehicle-intelligence cameras. These are cost figures in a presentation, not proof of expenditure or procurement approval.
- **Payment or deployment:** **Not established** by these records.

## Preserved proof and exact locators

| Claim | Official source and locator | Preserved SHA-256 |
|---|---|---|
| Matter 25-581 and final action, 6–3 | [Legistar event 2660](https://webapi.legistar.com/v1/coloradosprings/events/2660?EventItems=1&AgendaNote=1&MinutesNote=1&EventItemAttachments=1), `$.EventItems[38].EventItemMatterFile`, `$.EventItems[38].EventItemActionText`, `$.EventItems[38].EventItemPassedFlagName` | `f11a5712daf21f8f7d345d772995efb1cbeb495da7981bd1a1a6aa22b57a6759` |
| Nine individual votes on event item 103550 | [Legistar votes](https://webapi.legistar.com/v1/coloradosprings/EventItems/103550/Votes), `$[0..8].VoteEventItemId`, `$[0..8].VotePersonName`, `$[0..8].VoteValueName` | `99bbdb2394a1e30ad7761bea917a0700472604bab613b33b7ce4368327182c80` |
| Surcharge scope and direction of proceeds | [Signed Ordinance 25-93](https://coloradosprings.legistar.com/View.ashx?M=F&ID=14995655&GUID=00173B3A-1CD9-4839-9AD5-93B3C8E4EE03), PDF p. 1, Section 1, subsection (H)(1); final-passage date on p. 2 | `f364d09dbbb29a0b8d89c53002eb4bb757ddf0b942f245f4d70738543b4dc1fb` |
| Axon/Flock named as proposed major systems costs | [Police Technology Surcharge presentation](https://coloradosprings.legistar.com/View.ashx?M=F&ID=14876735&GUID=EA1466B1-D5D6-48CF-BB70-7710853E363D), PDF p. 7, “2026 Major Systems Costs” table | `7612843a72f957413f5eb7d0be71d27a47406e398816f80219c470e18387e6a8` |

The exact saved bytes are in [`fixtures/co/`](../fixtures/co/) and committed by [`SHA256SUMS`](../fixtures/co/SHA256SUMS). The individual-vote response was retrieved on November 9, 2026 UTC; the other records were retrieved August 16, 2026. The source links above are live official endpoints and may change later; the digests identify the preserved versions.

## Manual X draft for review

> Colorado Springs voted 6–3 on Nov. 25, 2025, to assess a police tech surcharge. A staff cost slide named Axon and Flock. No vendor contract is established by this vote. Which contracts, if any, use the proceeds? [decision page URL]

Replace the placeholder only after the decision page is live and its proof links have been checked. A person should review the wording and source links before posting. Do not use the older automated multi-post draft for this launch.

## Static atlas release workflow

The manifest at [`fixtures/atlas/colorado-springs-25-93.v1.json`](../fixtures/atlas/colorado-springs-25-93.v1.json) has `review_state: approved`, with one case-level release decision bound to its complete digest. A preview export remains a separate, gitignored editorial artifact:

```console
cargo run --locked -p pnull-cli -- atlas requirements
cargo run --locked -p pnull-cli -- atlas export --preview
```

Before public release, check the case and its proposed wording. The exporter checks each saved digest and JSON locator automatically. After the case check, change the manifest's `review_state` to `approved` and run `atlas requirements` again. Record the **one case-level release decision** with its displayed `id` and current `binding`:

```console
cargo run --locked -p pnull-cli -- atlas review \
  --id <requirement-id> --binding <displayed-sha256-binding> \
  --decision approve --reviewer <human-name> --note <review-rationale>
```

Record the separate structured publication permission:

```console
cargo run --locked -p pnull-cli -- atlas allow-release \
  --reviewer <human-name> --note <release-rationale>
cargo run --locked -p pnull-cli -- --site-url https://<actual-site-origin> atlas export
```

The production export atomically writes `web/panopticon-terminal/public/public-snapshot.json` only when all gates pass. It rejects demo reviewers, a pending or stale case-level release, a missing `atlas_release` allowlist, a changed source digest, and a placeholder site URL. It labels individual evidence `SOURCE_VERIFIED`, without implying per-record human approval. Check the decision page and proof links after deployment, then manually review and post the short X copy. No X token or automated posting is part of this launch.
