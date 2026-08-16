# Operator guide: citation review

Every public citation and image excerpt requires a human review decision before it may be published. The review queue is append-only and each decision is bound to exact content digests.

## States

A review decision has one of these states:

- **Pending** — no decision yet (or the most recent decision is not final). A citation without any decision is reported as `PENDING`.
- **Approved** — a human approved the citation for the proposed public fields.
- **Rejected** — a human rejected the citation.
- **NeedsContext** — a human judged that the citation needs additional context before it can be evaluated.
- **Superseded** — a later decision replaced an earlier one; history is preserved.

## Commands

### List the queue

```console
pnull review list
```

Lists every line citation and page citation together with its current decision state (`PENDING`, `Approved`, `Rejected`, `NeedsContext`, or `Superseded`) and its quote. Citations without a decision are shown as `PENDING`.

### Show a citation's decisions

```console
pnull review show <citation-id>
```

Shows all recorded decisions for a citation in order: decision ID, state, reviewer, note, decision time, and what it supersedes. If none exist, it reports that no review decisions exist.

### Approve a citation

```console
pnull review approve <citation-id> --reviewer <name> --note <text>
```

Records an `Approved` decision.

### Reject a citation

```console
pnull review reject <citation-id> --reviewer <name> --reason <text>
```

Records a `Rejected` decision.

### Supersede a decision

```console
pnull review supersede <decision-id> --reviewer <name> --reason <text>
```

Marks an existing decision as `Superseded` by appending a follow-up decision. This does not delete the prior decision; history is preserved.

## What binds a decision

When a review decision is recorded, the CLI computes a `ReviewBinding` from the citation's current values and stores its digest with the decision. The binding includes:

- evidence ID;
- source digest;
- locator/geometry (for a line citation, its locator; for a page citation, the page number);
- quote and quote digest;
- rule digest;
- processing-artifact digest (for a page citation, the text-map digest);
- proposed public fields.

The decision's `bound_digest` is the digest of that binding. If any bound value changes (for example, the quote, its geometry, the evidence digest, or the proposed public fields), the stored binding no longer matches, and the approval is invalidated.

## How the site, Atom, and X fail closed

Publication gates apply the review queue:

- A citation whose current decision is **not Approved** is not published. Pending, rejected, stale (a prior approval whose bound values no longer match), and mismatched decisions all block publication.
- The site, Atom feed, and X pipeline all apply the same gate, so a citation cannot leak through one channel while blocked on another.
- Image excerpts need a separate, explicit review gate and are not published automatically.
- Free-text reviewer notes are never published automatically.

## Demo behavior

The demo seeds clearly labeled deterministic demonstration reviews (for example, fixed-timestamp `Approved` decisions) so that the offline pipeline can exercise the publication path without claiming a real human reviewed the content. These are labeled as demonstration reviews and are never presented as real operator approvals.

## Notes

- The review queue is append-only: decisions accumulate and later decisions supersede earlier ones without deleting history.
- A citation that has never been reviewed shows as `PENDING` and is not published.
- Human review is a security and accuracy boundary, not a suggestion; publication of an unapproved citation is a failure of the pipeline.
