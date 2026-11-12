# Private CivicClerk PDF screening prototype

This explicitly invoked, offline prototype screens one PDF captured by the private CivicClerk list adapter. It revalidates the first-party government anchor, tenant portal and pinned client assets; the original zero-cursor list and exact selected event/file locator; and the utility and PDF receipts. Output identifies the source URL, locator and digest chain. Keep the separate input archives: their source bytes and receipts are not copied into the final result. The existing collector fetches at most one selected PDF per tenant per run; this prototype does not discover or fetch documents.

The output is a **private preliminary lead**, not a decision, responsible-body attribution, vote, or publication. The native-only extractor can omit scanned, embedded, hidden, malformed or image text, even when it returns text. `unverified_no_keyword_in_extracted_text` is not evidence of absence or complete document or jurisdiction coverage. Extraction failure, timeout, resource rejection, missing proofs, unsupported pagination, and empty output never become a no-match conclusion. No research database, public export, or case is written.

Run manually on Linux with a user systemd cgroup-v2 session, Bubblewrap and Poppler available, and a freshly built `pnull` from the same checkout. Both source and capture archives and the output parent must be absolute, real, owner-only directories, kept outside all served paths; do not publish, upload, or serve the result or input archives. `--seed-name` is optional and defaults to `private-probe-results-v3.json`. Only first-page zero-cursor captures are supported. The original output parent must have enough private disk space for a bounded source snapshot: each of the two input snapshots is capped at 64 MiB and 3,000 entries.

```sh
cargo build -p pnull-cli
python3 scripts/private-civicclerk-pdf-screen.py \
  --source-archive /absolute/private/seeds \
  --capture-archive /absolute/private/seeds/bounded-list-pass-01 \
  --output-parent /absolute/private/output \
  --output-name one-private-screen \
  --jurisdiction us-co-place-0856860 \
  --binary "$PWD/target/debug/pnull"
```

The wrapper uses a separate systemd scope capped at 512 MiB RAM, zero swap, 16 tasks, one CPU and 120 seconds; an outer Bubblewrap gives `/tmp` and `/archive` independent 128 MiB tmpfs mounts, and inherited limits restrict file size and descriptors. The Rust `--bounded-private` option independently checks cgroup and scratch limits and selects minimal devices, denies nested user namespaces and caps file output. The outer helper independently checks cgroup and mount bounds before invoking the extractor. If the kernel, user systemd or sandbox controls cannot be established, the screen fails closed. The resulting JSON includes the extracted native text and must remain private.

This is not a scheduled service or a production crawler. A hard crash or SIGKILL can leave a private `.civic-screen-*` snapshot in the output parent; an operator should remove only confirmed stale private scratch directories after checking that no run is active. The per-run caps do not impose an aggregate disk quota across repeated interrupted runs. Automatic scheduling requires managed private-storage quotas and independent operational review. No same-owner process should concurrently alter the trusted binary, code, rules or operating environment.

Tests: `python3 scripts/test-private-civicclerk-pdf-screen.py`; `cargo test -p pnull-ingest sandbox::tests`; and, where user systemd is available, `PNUL_TEST_PDF_BINARY="$PWD/target/debug/pnull" python3 scripts/test-private-civicclerk-pdf-screen.py`. The latter runs a synthetic mixed image/native fixture, checks unbounded direct invocation fails, and checks that a short scope deadline tears down its descendant. It does not establish the reliability of all possible PDF extraction.
