# State bot architecture

`StateConfig` separates state code/name, jurisdiction, canonical site URL, source list, feed label, rate policy, and fixture metadata from application logic. Version 0.0.1 ships only `configs/states/co.toml`; no other state is implied or scaffolded.

The X component accepts a state-derived alert and canonical base URL. It does not know Colorado-specific detection logic. A draft:

- identifies itself as automated;
- names the jurisdiction and monitored matter;
- states the current public-record state without claiming that an ordinance proves a vendor purchase;
- gives the detected change;
- links the local alert page, which contains citations and hashes;
- fits 280 Unicode scalar values per post and emits a short numbered thread when necessary.

Drafting is dry-run by default. Approval stores a digest over alert ID and every exact post. Posting checks that digest, local approval, explicit `--confirm`, a non-placeholder canonical URL, and runtime credentials. The trait-backed transport is replaced by a fake in tests. Duplicate or uncertain attempts are fail-closed.

A future state feed should add one reviewed configuration and source adapter fixtures. It must not copy the publication or transport logic. Account creation, registration, scheduling, and credential provisioning are outside this project and are never automated.
