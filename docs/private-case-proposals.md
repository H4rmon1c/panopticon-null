# Private source-checked case proposals

`pnull case propose --input proposal.json` turns one to eight archived official
document candidates into a **private proposal**. It does not add a case to the
atlas, a feed, or an X draft. A candidate URL or a keyword match is never a
decision, vote, contract award, or person's role by itself.

Use the jurisdiction config that archived the candidate. For example, after
the reviewed Moffat index and a PDF under its fixed `/uploads/` path have been
captured:

```sh
pnull --config configs/states/co-moffat.toml \
  --data-dir /var/lib/panopticon case propose --input proposal.json
```

The input is JSON with `schema_version: 1`, the `jurisdiction_ref` from
`co-document-index-geography.v1.json` or `co-calendar-geography.v1.json`, and
`sources` entries. Each source entry has a local `source_ref` and an
`archive_record` relative to the private data directory, such as
`discovery-archives/moffat-pdf-documents/<archive-key>.json`. The archive key is
printed by `source archive-candidate` or `source archive-queue`. It must be a
record created by those commands; a URL alone is not accepted.

All factual fields are optional, but `votes` and `unresolved_question` are
required. A factual field has this form:

```json
{
  "value": "the exact text asserted",
  "proof": {
    "source_ref": "minutes",
    "locator": "line:17",
    "quote": "the exact text asserted"
  }
}
```

Scalar claims must equal the complete extracted line at `line:N`. This
conservative rule prevents a substring such as “approved” from reversing
“not approved.” The builder reopens the original
raw bytes, verifies their SHA-256, the matching public-network fetch receipts,
current source-level reviews, the index anchor, the official URL path policy,
and the extracted locator. Missing fields become explicit `not_established`
states. The output binds each claim to source URL, digest, evidence ID, receipt,
quote, and locator, and saves a deterministic JSON file under private
`case-proposals/` with mode `0600`.

The optional `decision` object has `matter`, `date`, `body`, `action`, and
`outcome` claims. The other optional claims are `technology`,
`technology_relationship`, and `contract_reference`. A
`technology_relationship` must be an atomic line such as
`Item 5: Flock Safety` that names the already cited matter and technology;
it must be in the archived source that establishes the matter, action, and
outcome, within one bounded item section. If the item heading repeats in a
multi-meeting document, the builder refuses the link. Without it the output
remains a screening proposal. A `contract_reference`
only establishes that the cited text mentions a contract. The
`contract_award_link` output stays
`not_established` until a separate, purpose-built award or approval chain can
verify it. Global role, term, and portrait claims are rejected because the
current input has no named official subject; use the named voter fields below.

The `votes` object uses `scope` `not_established`, `direct_item`, or
`consent_motion`. `not_established` must have no motion or rows. Named rows
require a cited motion, an atomic name and position in each row's quote, and
the same source as the motion. Direct item votes additionally require the
complete motion line to name the exact matter, each row to follow within 25
extracted lines, one unique matter heading in the archived source, and no
intervening item or motion heading. A consent motion can record named votes on
the **consent motion** only when its complete line names the cited matter and
consent scope in the same source. `item_votes_established` remains false. Each named
row may separately cite the person's public `role`, `term`, and official
`portrait` on an atomic line naming that person; absent links are
`not_established`. An aggregate phrase such as
“all in favor” cannot create named rows.

To correct a private proposal, provide both `supersedes_proposal_id` and
`correction_note`. The new immutable proposal records the prior artifact's
digest and carries its correction history. Source review is per endpoint or
path policy, not per record, but release still requires a separate publication
path with source-backed link checks and privacy screening. No proposal is
automatically published. A correction cannot cross jurisdictions.

The Moffat index fixture only tests the bounded official PDF link path. It is
not a finding that Moffat adopted a surveillance technology, awarded a
contract, or recorded an individual vote. The same schema applies to the
reviewed Legistar paths and other towns after their exact source policies and
geography mappings have been added.
