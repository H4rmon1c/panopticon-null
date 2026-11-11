# Atlas snapshot export

The first atlas case is the versioned manifest at `fixtures/atlas/colorado-springs-25-93.v1.json`. It records Colorado Springs Ordinance 25-93, final passage on November 25, 2025, and the official 6–3 roll call. The ordinance amends city code to assess a Police Department Technology Surcharge. A named vendor contract or award linked to this vote is **not established** by the checked records. The recorded voters' individual office roles and terms are also **not established** by these sources.

The Rust exporter reads this manifest and checks every cited local fixture against its expected SHA-256. For JSON records, it also checks exact locators and recorded names, votes, matter, event item, and tally. The output contains short claims and source metadata, not raw records or private reviewer notes. Map coordinates are approximate jurisdiction display anchors, not evidenced site locations.

## Private preview

From the repository root:

```sh
cargo run -p pnull-cli -- atlas requirements
cargo run --locked -p pnull-cli -- atlas export --preview
```

The preview has `preview:true`, `demo:false`, and `PENDING_RELEASE` evidence states. The atlas only reads it through its explicit `?preview=1` route. Keep `preview-snapshot.json` out of the public PR and public deployment. A synthetic manifest, if used in a preview, is marked `demo:true` and cannot pass production export.

## Legacy Colorado Springs release gate

Production export requires all of these, in order:

1. Set the manifest's `review_state` to `approved` after checking the cited case and public wording.
2. Run `atlas requirements` again. It returns **one case-level release binding** for the complete manifest, not a task for each vote or source claim.
3. Record the case-level human decision once with `atlas review --id ... --binding ... --decision approve --reviewer ... --note ...`. A later rejection or changed manifest binding blocks release.
4. Record the `atlas_release` publication allowlist once with `atlas allow-release --reviewer ... --note ...`.
5. Export with an explicit non-placeholder HTTPS origin: `cargo run --locked -p pnull-cli -- --site-url "$PNUL_SITE_URL" atlas export`.

Set `PNUL_SITE_URL` to the actual deployment origin. The exporter verifies every preserved byte digest, JSON locator, vote, identity, and privacy rule automatically before atomically replacing the output. The public snapshot has `preview:false`, `demo:false`, `SOURCE_VERIFIED` evidence, case-level release metadata, and a shareable `decisions/co-colorado-springs-25-581-2025-11-25.html` page with its own crawler metadata. The generic `decision.html` route supports listing, preview, demo, and old query links; its metadata is neutral and marked `noindex`. `SOURCE_VERIFIED` does not claim a person approved each individual record.

The staff cost presentation names Axon and Flock systems as context. Those records do not establish that this ordinance approved a named vendor contract, award, or payment. The static graph therefore has no contract-approval edge. It keeps technology context and an explicit unresolved question for later research.

## Version 2 Colorado cases

The source-verified manifests in `fixtures/atlas/cases/` cover Commerce City Ordinances 2755 and 2756, Castle Rock Resolution 2025-071, and Denver's **failed** Flock Resolution 25-0413. Four consent items are pending under `fixtures/atlas/preview-cases/`: Arapahoe County 24-633 and 25-378, and Denver Axon 25-1967 and 25-1968. They show each named vote on the consent motion as block-level context while keeping the item-specific voter list empty; proposed agreement amounts do not establish payments. Repeat `--manifest` to combine them with the legacy Colorado Springs case in a preview snapshot. A preview validates all local source bytes, exact JSON/HTML locators, complete direct or block-motion roll calls, jurisdiction roster IDs, and cautious contract relationships, but it is never a public release.

The Legistar item-votes API is an evidence fallback when a document does not supply named positions. A version 2 decision may use `vote_record_state: "not_established"` only when the preserved response from that decision's **exact, unfiltered** `EventItems/{id}/Votes` URL is an empty JSON array. Query parameters on item-vote endpoints are rejected: a filtered response cannot prove that a roll call is complete or empty. The decision remains visible with no invented voter names and an explicit gap note; an empty API response does not prove that no vote occurred or that no other public record has the names. A nonempty response cannot be hidden as a gap. Consent items must use the separate block-motion model.

The version 2 source gate now binds each case's Colorado geography ID to one of seven explicitly configured Legistar clients, including Douglas County and Littleton. Event, matter, and item-vote API paths are narrow, and matching portal meeting/document pages are accepted as preserved sources. Additional `fixtures/co-<slug>/` directories are allowed only with normal path components and canonical containment inside `fixtures/`. A different Legistar client cannot be used to prove a case for this jurisdiction. Other public-meeting platforms still need their own source adapter and claim verifier.

The checked-in [four-case static release](four-case-release.md) contains Colorado Springs, the two Commerce City ordinances, and Denver's failed resolution. Castle Rock and the four consent cases remain outside the public snapshot.

Version 2 production does not require a human decision on each case. It requires the global `atlas_release` allowlist, a current source-policy review for each cited endpoint, private HTTPS fetch receipts from the exact reviewed URL, and a raw blob whose SHA-256 and bytes match the case fixture. An explicit case rejection or correction vetoes automatic release. A changed source response also blocks the old case until its claims are checked again. The 12 cited endpoints now have current reviews and one preserved live capture each. Eleven responses exactly match the fixtures; Castle Rock's detail HTML has a different raw digest despite only dynamic ASP.NET state and resource timestamp changes, so an exact-byte release still needs a reviewed revision. See the [capture result](colorado-source-capture-results.md). Five attachment URLs on robots-disallowed hosts were removed from release citations after checking that no released fact depends on them; their historical fixtures remain outside the release set. The fixed endpoints are listed in [the archive profiles](colorado-archive-profiles.md); source reviews are separate from case review.

`deploy/selfhost/publish.sh` takes the space-separated `PNUL_MANIFEST_PATHS` list, exports into a staging directory, generates a separate static decision page with case-specific social metadata, validates the release, and then switches the public symlink. `PNUL_WITHDRAWN_MANIFESTS` can name exact, currently rejected manifests; `PNUL_WITHDRAWN_CASE_IDS` carries rejected proposal cases from the previous public snapshot. Both receive noindex correction notices at their old share URLs without private reviewer notes. A failed export leaves the previous release live. The public browser fetches only this static dataset and the locally bundled geography file.

The new `case release` bridge rechecks an original document candidate proposal and emits a single-case source-verified snapshot. `atlas export --proposal-input` can add that case to the manifest bundle under the proposal's jurisdiction config. It supports exact direct votes and narrow award-authorization lines, while preserving missing links as gaps. See [private case proposals](private-case-proposals.md).
