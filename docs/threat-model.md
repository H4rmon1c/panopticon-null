# Threat model

## Protected interests

- Integrity and provenance of original public evidence.
- Accuracy and restraint of published claims.
- Privacy of people incidentally present in public records.
- X credentials and operator approval intent.
- Availability of the local workstation and bounded use of CPU, memory, process, and disk resources.
- Reproducibility of rules, builds, and output.

## Adversaries and failures

Sources may serve malformed HTML, JSON, PDFs, decompression bombs, huge pages, misleading metadata, scripts, macros, attachments, redirects, or later-edited documents. Vendors or authorities may remove language, change files in place, or dispute characterization. An operator may accidentally publish sensitive free text or approve one draft and later generate another. A partial network failure may make the state of an X thread uncertain.

## Controls

- HTTPS only, same-host HTTPS redirects only, literal private/loopback destinations rejected, bounded redirects and response bytes.
- Manual current-robots review required for live ingestion; a persisted minimum source interval prevents repeated calls.
- Original-byte hashing, collision-safe identifiers, existing-blob verification, private local directory permissions, and duplicate prevention.
- No browser engine, script execution, shell interpolation, macros, embedded attachments, or source-driven commands.
- Poppler/Tesseract allowlist; cleared environment; page, OCR page, raster dimension, address-space, CPU, file, process, output, and timeout limits.
- Exact citations, rule digest, negation handling, ambiguity fallback, and rhetoric separated from factual claims.
- Minimal meaningful diff lines rather than raw free-text republication.
- Shared privacy checks for static and X text; explicit local approval remains required.
- X credentials only from environment or a mode-`0600` file; generic errors never include request/response bodies or tokens.
- Approval binds to the exact post/thread digest. Per-segment remote IDs are persisted.

## Residual risk

DNS names are not resolved and pinned before a request, so the literal-address check is not a complete defense against DNS rebinding. External PDF parsers remain complex native code; process limits are not a kernel sandbox. Privacy pattern checks cannot reliably distinguish every person's name or every plate format. Human publication review is therefore a security boundary, not a suggestion.

The optional OCR budget applies limits to each allowlisted process and caps OCR at five pages; it is not a single kernel-enforced job deadline. v0.0.2 should add an operating-system sandbox, aggregate temporary-disk accounting, DNS resolution policy, and structured publication allowlists.
