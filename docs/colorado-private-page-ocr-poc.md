# Private page OCR proof of concept

This prototype extracts native text and renders/forces OCR of **one original PDF page** per invocation, including pages that already have native text. It is for private research only. It never writes `document_candidates`, changes coverage counts, creates a public record, or emits a no-match result. OCR can miss or misread material even when it succeeds; page output is not evidence that all content was understood.

The current whole-document extractor returns as soon as it finds any native text, skipping OCR for other pages, and its multi-page OCR acceptance checks only aggregate nonempty output. The synthetic `mixed-native-image.pdf` and `empty-ocr-page.pdf` fixtures reproduce those limitations; their original content is synthetic. The POC demands a valid native page boundary, a successful sandboxed render, and a nonempty OCR map for the exact requested page. Missing, empty, failed, or timed-out OCR returns an error and creates no page result. A legitimately blank page is unresolved, too. It does not expose or claim publication-quality geometry.

The wrapper accepts only a **private archive already created by the large-PDF private screener**, and rechecks the PDF digest, direct index anchor, and PDF/index receipts before every run. The private root and every archive-path component, as well as source, receipt, and result directories, are opened without following symlinks; archive reads and page writes are anchored to directory descriptors, and the pinned content directory is passed into the worker. A coordinating process may additionally pass both `--private-root-fd` and `--archive-fd` as inherited FDs; the worker compares their device/inode identities against no-follow walks of the explicitly declared paths, and rejects replacement before processing. Redirect and multi-index chains remain unsupported. It saves one immutable 0600 JSON file per page under `ocr-pages/`, using an atomic link and recording the original source digest, source/index candidate, exact page number and count, native and OCR text, method, executable hashes and language-data hash. Rerunning the same page repeats native extraction and OCR, verifies the entire output against the existing page, and fails closed on any discrepancy; source, method and toolchain hashes must agree. Different toolchain versions require a fresh archive. Shared libraries and all runtime configuration are not fully fingerprinted; these artifacts are experimental and need independent review.

```sh
python3 scripts/private-research-page.py \
  --archive /absolute/private/large-pdf-archive \
  --private-root /absolute/private/owner-only-root \
  --binary /absolute/path/to/pnull \
  --tessdata /absolute/path/to/english/tessdata \
  --page 137
```

The jurisdiction value is an unverified source-store label; the wrapper does not establish agency attribution. The POC limits PDFs to 20 MiB and 300 pages. Each page runs in a dedicated systemd user scope. Before extraction pnull checks the actual cgroup v2 limits: at most 512 MiB memory, zero swap, 16 tasks and one CPU. The scope has a 60-second runtime limit, and the wrapper has a 90-second deadline. An outer Bubblewrap namespace uses a 128 MiB tmpfs for private temporary work, which vanishes on teardown; pnull independently checks the actual `/tmp` mount before extraction, so an unbounded direct invocation fails. A page-only inner sandbox disables nested user namespaces and writable directories, permits only the exact precreated render/OCR output file, excludes PTYs, limits file descriptors, and uses parent-death teardown. It processes pages serially through separate invocations, allowing interruption and resumption at the next page; revisiting a page always re-extracts it. It does not yet schedule a full packet or assert that all pages have been processed. Native and OCR text can contain private information. The operator must choose an owner-only private root that is not served by any web server; known deployment roots are blocked, but arbitrary served roots cannot be discovered automatically. Source attribution and any publication require the existing evidence and review pathways.

With the development binary and installed English Tesseract data, run:

```sh
PNUL_TEST_BINARY=/absolute/path/to/pnull PNUL_TESSDATA_DIR=/absolute/path/to/english/tessdata \
  python3 scripts/test-private-page-ocr.py
```

To run the explicit before/after regression as well, set `PNUL_TEST_LEGACY_BINARY` to a separately retained pre-POC build. The legacy binary is expected to report the mixed fixture as a native-text no-match; the POC must recover the image-only phrase on page 2. This optional test documents a historical defect and should not be pointed at a later fixed binary.

**Scope:** These stricter controls apply only to the private page worker. The existing general whole-document extractor still permits a writable host `/work` directory and lacks an aggregate scratch quota. The POC remains private and unscheduled pending independent security review and a reliable way to verify source attribution and any future whole-document coverage.

Integration tests skip when the required binary, English language data, systemd user scope, or Bubblewrap user-namespace support is unavailable. Deterministic Rust checks still run. The local full test above requires cgroup v2 and a functioning user systemd manager.
