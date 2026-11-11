# Private screening of archived oversized packets

This opt-in, offline tool screens **one already captured** municipal or county PDF that the normal collector left unresolved because it exceeded the 100-page limit. It checks the exact source-index anchor and both PDF and index receipts, preserves their original bytes in a fresh private directory, and never writes the research database or export rows. A mention is a preliminary lead, not a decision, contract, vote, or official attribution.

```sh
python3 scripts/colorado_large_pdf_private.py \
  --source-dir /absolute/private/research \
  --private-dir /absolute/fresh/private/screen \
  --jurisdiction us-co-place-0810105 \
  --url https://www.buenavistaco.gov/AgendaCenter/ViewFile/Agenda/_01092024-1704
```

The input must have exactly one archived successful PDF digest for the URL; ambiguity is rejected. Only direct index-to-PDF candidates are accepted; redirect and Colorado Springs multi-index chains are rejected until their complete provenance can be preserved. The index binding is rechecked against the newly copied private archive. The output has 0700 directories, 0600 files, no database, and must never be served as a website. It stores page numbers and extracted text line numbers for candidate mentions and keeps the exact source and index digests, candidate anchor, and both receipts. Existing normal publication validation remains unchanged.

The sandbox has no network or user-home mount. It requires Linux `bwrap`, Poppler `pdfinfo`, `pdfimages`, and `pdftotext` at `/usr/bin`; each command has a 30-second wall limit, 20-second CPU limit, 1.5 GiB address space, and 8 MiB output limit. PDFs must be 101–300 pages, at most 20 MiB, and are extracted in segments of 25 pages. A malformed page count, warnings, invalid page mapping, empty page, image on any page (even a logo), timeout, image inventory failure, or unavailable tool yields `incomplete` and exit code 2; it never produces a no-keyword result for that packet. The strict image rule deliberately holds many normal packets for OCR or review. Even a full text extraction can omit information, so `private_extracted_text_no_keyword` is not proof that a document contains no relevant material.
