# Historical CISC membership source

The Colorado General Assembly's 2019 hearing summary links a one-page PDF
headed **2018 CISC Member Agencies**, revised November 2018. The PDF lists 66
numbered rows and says additional memberships were pending. It establishes a
historical list only: it does not establish present membership, use of an
individual technology, a contract, a purchase, or any official's vote. Some
rows name state or federal agencies or judicial districts rather than a single
municipality. These are private OCR candidates, not public atlas cases.

`scripts/colorado_cisc_2018_members.py` captures the committee page and checks
the exact labeled link to the attachment. It archives the original HTTP
receipts and bytes, requires the pinned PDF SHA-256
`006e916f7719b0f298d9688a4462eac14d03847b20f6f223d0158f460d5515e3`, and
uses the existing isolated PDF extractor with OCR and verified page mapping.
The 66 source labels were manually pre-transcribed from the pinned page; this
is a source-specific transcription rather than autonomous agency discovery.
The parser accepts each pre-transcribed row only if OCR text uniquely supports
the row; unrecognized or ambiguous rows are explicitly omitted. Each
candidate retains a page/column/row locator and both source and OCR digests.
The machine-generated text can be wrong and must not silently create a public
claim. Every candidate is labeled `manual_pretranscription_ocr_matched` rather
than verified. A changed source digest or missing header/qualifier stops the parser.

Run in a fresh private archive with a built `pnull` binary, Bubblewrap,
Poppler and Tesseract. Set `PNUL_TESSDATA_DIR` to a real, non-symlinked
English tessdata directory inside `/usr` or `/nix/store` as required by the
sandbox. On November 11, 2026 the attachment path was excluded by robots; the
project owner explicitly authorized ignoring that exclusion. To apply that
owner instruction to this exact attachment, pass the narrowly scoped flag:

```sh
PNUL_TESSDATA_DIR=/nix/store/EXAMPLE-tessdata \
  python3 scripts/colorado_cisc_2018_members.py \
  --data-dir /absolute/private/path --pnull-bin /absolute/path/to/pnull \
  --allow-attachment-robots-override
```

Without the flag a disallowed attachment is not fetched. The output records
whether the override was actually used. It is a mode-0600 file named
`cisc-2018-private-ocr-candidates.json`, which the command refuses to
overwrite. Raw responses, OCR output, and network observations stay in the
private archive; this command does not alter public research or alert feeds.

Tests: `python3 scripts/test-colorado-cisc-2018-members.py`.
