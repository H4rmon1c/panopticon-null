# Security policy

## Reporting

Do not open a public issue containing credentials, unpublished personal records, exploitable source documents, or a privacy bypass with real sensitive values. Until the project publishes a dedicated security address, use the repository host's private security-advisory feature. If none is available, report only a minimal non-sensitive description publicly and request a private channel.

Never send X tokens, secret files, raw plate data, movement histories, home addresses, or private identities with a report.

## In scope

- Evidence corruption, provenance confusion, digest or identifier failures.
- HTML/XML injection, unsafe URL handling, or sensitive-data publication bypass.
- Hostile-document escapes, unbounded extraction, command execution, or sandbox failures.
- Credential exposure, approval bypass, arbitrary-draft posting, or duplicate posting.
- SSRF and DNS-rebinding defenses, mixed-answer fail-closed behavior.
- Conditional-retrieval handling (304 must never create a new blob for unchanged content).
- Review-subversion: changing a bound value must invalidate an approval.
- Procurement-conflation: an action must never be transferred to another subject.
- X reconciliation bypass, audit-history deletion, or blind retry of uncertain attempts.
- Deterministic classification bugs that create unsupported strong claims.
- Supply-chain and reproducibility failures.

## Security posture at 0.0.2

- **Sandbox.** Live PDF/OCR extraction runs in a Linux bubblewrap sandbox: no network namespace, no inherited secrets, no writable access outside a dedicated temporary output directory, read-only exact inputs, new process/session boundaries, and prlimit CPU/memory/file-size/output-size/wall-time limits. Cleanup happens on success, failure, timeout, or interrupt. Live ingestion fails closed when the sandbox cannot be established. Aggregate job budgets bound downloaded bytes, attachments, PDF pages, OCR pages, extracted bytes, child processes, CPU, and wall time.
- **DNS-safe HTTP.** Only public addresses are accepted; private, loopback, link-local, multicast, unspecified, and documentation addresses are rejected, and mixed public + prohibited DNS answers fail closed. HTTPS is required; certificate validation cannot be disabled. Redirects are same-host. Provenance is persisted for every request and redirect.
- **No secrets.** HTTP provenance never persists cookies, authorization headers, or bearer tokens. Processing-run environment capture uses an allowlist, not an environment dump. Subprocess logs contain no secrets. Credentials are never placed in command arguments, source, fixtures, logs, shell examples, or generated output.
- **Publication gates.** Every public citation and image excerpt requires a human review decision bound to exact content digests. Publication allowlists are permissions, not auto-approval. The site, Atom, and X fail closed on pending, rejected, stale, or mismatched decisions.
- **No content execution.** Source-document content is never executed: no macros, embedded files, JavaScript, or document actions.

## Operational guidance

Keep `.pnull` and secret files on an encrypted local filesystem. Use a mode-`0600` runtime token file. Never put credentials in command arguments, source, fixtures, logs, shell examples, or generated output. Review generated citations and diffs before publication. The default `.invalid` canonical URL intentionally prevents live posting until an operator configures and reviews a real public site.

Maintain current source reviews (`pnull source review capture/record/show/verify`) before live retrieval; live retrieval refuses without a current review.

If a post attempt fails after any segment, do not clear the reservation or retry blindly. Inspect X and the persisted `post_segments` state, then reconcile manually via `pnull x reconcile`.

Supported security updates apply to the latest tagged release.
