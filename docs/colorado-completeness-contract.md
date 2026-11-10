# Colorado public-record coverage contract

The coverage target is every Colorado public body that can make a relevant surveillance-technology decision, regardless of population. A town with 200 residents belongs in the same source queue as a major city. The current geography roster is a starting index, not a complete list of agencies, tribal governments, universities, authorities, or overlapping districts.

## What each public record must show

- **Decision:** the public body, exact matter identifier, date, action, outcome, technology, contract or award connection when established, and a link to each preserved source with an exact locator and SHA-256 digest. An ordinance mention does not become a vendor award.
- **Vote:** every named position the official record supplies, tied to that exact decision or motion. A direct item roll call is shown on the item. A consent-block roll call is shown as block context and never assigned to an individual consent item. An aggregate phrase such as “all in favor” does not establish each person's vote without a named roster and an explicit voting record.
- **Official:** use a stable identity key from the jurisdiction and official record, not a name match across towns. Role and term require their own source. Preserve an official portrait's original bytes and digest and show both a local preview and access to the uncropped original when identity and provenance are established. A missing public portrait is marked “not established”; no image is inferred or generated.
- **Gap or correction:** a relevant decision can be shown with a missing vote, contract, signature, payment, role, or portrait marked “not established.” Later source revisions and corrections must remain visible without overwriting the earlier record.

## Acquisition order

1. Archive the public body's official HTML index, agenda, minutes, contract, and award documents with bounded HTTPS reads and immutable raw bytes.
2. Use an official API when a document omits structured vote rows, to catch a missing document, or to check a changed record. Preserve the API response and exact row locator too. If documents and API disagree, hold the disputed fact for reconciliation instead of silently choosing one.
3. Publish only facts supported by current source-policy review, raw-byte digests, private fetch receipts, and the case's evidence links. This is a source-level gate; it does not require a person to approve every record.

The public atlas reads the local published dataset. It does not call municipal APIs when a visitor opens a map or decision page.

## Coverage accounting

Report these separately for each jurisdiction: official source discovered; source policy reviewed; index captured and last checked; relevant matters found; evidence-backed decisions published; direct named votes; consent-block votes; votes not established; verified original portraits; portraits not established; contract/award links established; and unresolved corrections. A map entry without a published case says “Records not indexed,” not “no surveillance.” Statewide launch claims require a sourced inventory of public bodies and a measured lookback period, not just a populated geography map.
