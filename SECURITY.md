# Security policy

## Reporting

Do not open a public issue containing credentials, unpublished personal records, exploitable source documents, or a privacy bypass with real sensitive values. Until the project publishes a dedicated security address, use the repository host's private security-advisory feature. If none is available, report only a minimal non-sensitive description publicly and request a private channel.

Never send X tokens, secret files, raw plate data, movement histories, home addresses, or private identities with a report.

## In scope

- Evidence corruption, provenance confusion, digest or identifier failures.
- HTML/XML injection, unsafe URL handling, or sensitive-data publication bypass.
- Hostile-document escapes, unbounded extraction, command execution, or sandbox failures.
- Credential exposure, approval bypass, arbitrary-draft posting, or duplicate posting.
- Deterministic classification bugs that create unsupported strong claims.
- Supply-chain and reproducibility failures.

## Operational guidance

Keep `.pnull` and secret files on an encrypted local filesystem. Use a mode-`0600` runtime token file. Never put credentials in command arguments, source, fixtures, logs, shell examples, or generated output. Review generated citations and diffs before publication. The default `.invalid` canonical URL intentionally prevents live posting until an operator configures and reviews a real public site.

If a post attempt fails after any segment, do not clear the reservation or retry blindly. Inspect X and the persisted `post_segments` state, then reconcile manually.

Supported security updates apply to the latest tagged release.
