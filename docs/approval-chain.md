# One documented approval chain: Chino Axon agreement

This is a source-bound **draft ledger**, ready for human review. It is a model for a future public website, not a public allegation page. The chain follows conduct in a public office. It records what the sources establish and carries each missing link as “not established.”

## The documented sequence

1. [Chino matter 26-308](https://webapi.legistar.com/v1/chino/Matters/3139) describes a proposed five-year agreement with Axon Enterprise Inc. for “Axon Basic with Full AI,” with an amount of $4,464,008.92 and a first-year appropriation of $799,495.18. Its status is Passed.
2. At the [June 16, 2026 City Council meeting](https://webapi.legistar.com/v1/chino/Events/1961), [event item 52243](https://webapi.legistar.com/v1/chino/Events/1961/EventItems/52243) lists that matter as **Consent Calendar item 9** and marks it approved.
3. [Event item 53661](https://webapi.legistar.com/v1/chino/Events/1961/EventItems/53661) records a motion to approve **Consent Calendar items 1–14 as presented**. Its action passed. This is the decision connected to the specific Axon item through item 9's recorded consent status and approval.
4. The [roll-call response for event item 53661](https://webapi.legistar.com/v1/chino/EventItems/53661/Votes) records five AYES votes: Eunice M. Ulloa, Curtis Burton, Karen C. Comstock, Christopher Flores, and Marc Lucio. The same motion records Burton as mover and Lucio as seconder. The votes are votes **on the consent-calendar motion**, not a separate Axon-only roll call.
5. Official city pages establish each person's public role and published term years: [Ulloa](https://cityofchino.org/m/directory/employee?eid=40) (Mayor, 2024–2028), [Burton](https://cityofchino.org/580/Mayor-Pro-Tem-Curtis-Burton) (Mayor Pro Tem, District 2, 2022–2026), [Comstock](https://cityofchino.org/518/Karen-Comstock) (Council Member, District 4, 2024–2028), [Flores](https://cityofchino.org/519/Christopher-Flores) (Council Member, District 1, 2024–2028), and [Lucio](https://cityofchino.org/517/Marc-Lucio) (Council Member, District 3, 2022–2026).

The ledger calls the agreement **authorized**. It does not call it executed. It records all five votes separately and keeps moving and seconding separate from voting.

## Per-connection evidence and review

Each connection is an immutable `ApprovalLink` with a type, two record IDs, source proofs (URL, field locator, quote, and SHA-256 digest of preserved response bytes), and a binding digest over the complete claim. The consent-to-contract link cites both the item-9 record and the consent motion. Office role and term span have separate exact locators. The importer validates the matter, event, consent item, vote, and person IDs together and joins each City office profile through its public office email. It saves the exact JSON and HTML responses in the content store and records an initial **Pending** review on every link. The [fixture manifest](../fixtures/approval/SHA256SUMS) gives the hashes for the committed source snapshots.

`ApprovalReview` is append-only. Every later review names the prior review it supersedes and binds to the current link digest. `ApprovalCorrection` is also append-only: it records an operator, reason, source proof, and whether the old link was clarified, retracted, or replaced. Retraction or replacement removes a previously approved connection from the current chain without deleting its history. The CLI `approval show` displays both histories.

```console
# Run from the repository root. Keep the data directory private.
cargo run --locked -p pnull-cli -- --data-dir .pnull approval import-chino
cargo run --locked -p pnull-cli -- --data-dir .pnull approval show <contract-id>

# Inspect each source snapshot and binding before reviewing it.
cargo run --locked -p pnull-cli -- --data-dir .pnull approval review <link-id> \
  --binding <binding-digest-from-show> --decision approve --reviewer NAME --note NOTE

# A correction requires a preserved source digest and exact locator/quote.
cargo run --locked -p pnull-cli -- --data-dir .pnull approval correct <link-id> \
  --kind retracted --operator NAME --reason REASON \
  --source-url <official-url> --snapshot-digest <preserved-digest> \
  --locator <exact-locator> --quote <source-quote>
```

Reviewing one connection does not auto-approve the others. The current chain projection includes only approved, unretracted links. A future website exporter must additionally review public display fields before publishing; the importer and CLI do not publish this chain.

## Not established

- Exact start and end **dates** within each published office term-year span.
- A signed or otherwise executed Axon agreement. The council decision authorizes the purchase; no executed instrument is preserved in this case.
- Payment of the quoted amount or any installment.
- Installation, deployment, actual use, or effects of the technology.
- Any relationship between this Chino decision and Colorado Springs procurement matters.

These gaps are first-class records tied to the source digests checked. They should remain visible in any future public page until a cited, reviewed source resolves them.
