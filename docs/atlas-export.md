# Atlas snapshot export

The first atlas case is the versioned manifest at `fixtures/atlas/colorado-springs-25-93.v1.json`. It records Colorado Springs Ordinance 25-93, final passage on November 25, 2025, and the official 6–3 roll call. The ordinance amends city code to assess a Police Department Technology Surcharge. A named vendor contract or award linked to this vote is **not established** by the checked records. The recorded voters' individual office roles and terms are also **not established** by these sources.

The Rust exporter reads this manifest and checks every cited local fixture against its expected SHA-256. For JSON records, it also checks exact locators and recorded names, votes, matter, event item, and tally. The output contains short claims and source metadata, not raw records or private reviewer notes. Map coordinates are approximate jurisdiction display anchors, not evidenced site locations.

## Private preview

From the repository root:

```sh
cargo run -p pnull-cli -- atlas requirements
cargo run --locked -p pnull-cli -- atlas export --preview
```

The preview has `preview:true`, `demo:false`, and pending evidence states. The atlas only reads it through its explicit `?preview=1` route. Keep `preview-snapshot.json` out of the public PR and public deployment. A synthetic manifest, if used in a preview, is marked `demo:true` and cannot pass production export.

## Public release gate

Production export requires all of these, in order:

1. Set the manifest's `review_state` to `approved` after checking the cited records and public wording.
2. Run `atlas requirements` again. Its 14 claim rows and separate release row show the current review IDs, exact bindings, source URLs, locators, and SHA-256 values.
3. Record a human review for every current claim binding and for the release binding using `atlas review --id ... --binding ... --decision approve --reviewer ... --note ...`. Any later rejection, supersession, or changed binding blocks release.
4. Record a separate `atlas_release` publication allowlist using `atlas allow-release --reviewer ... --note ...`.
5. Export with an explicit non-placeholder HTTPS origin: `cargo run --locked -p pnull-cli -- --site-url "$PNUL_SITE_URL" atlas export`.

Set `PNUL_SITE_URL` to the actual deployment origin. The exporter validates all gates before atomically replacing the output. The public snapshot has `preview:false`, `demo:false`, reviewed evidence, and a shareable `decision.html?decision=co-colorado-springs-25-581-2025-11-25` link.

The staff cost presentation names Axon and Flock systems as context. Those records do not establish that this ordinance approved a named vendor contract, award, or payment. The static graph therefore has no contract-approval edge. It keeps technology context and an explicit unresolved question for later research.
