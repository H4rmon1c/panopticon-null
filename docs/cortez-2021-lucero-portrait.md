# Cortez historical portrait evidence

An isolated private discovery capture preserved the City of Cortez's *2016
Annual Report and 2017 Budget in Brief*, original PDF SHA-256
`ab67c9832615adda8ac2dd6e31023476c753002704bb0eedb84a926f826e3023`, from
`https://www.cortezco.gov/DocumentCenter/View/775/2017-Budget-in-Brief?bidId=`.
Its physical PDF page 2 (printed page 1) labels the lower-left portrait
“Orly Lucero, Councilmember.” A visual inspection of that exact PDF established
the label and image position; no automated face recognition is involved.

The original embedded JPEG is PDF object 7 0, SHA-256
`792f36327372957b5cd20b24fa7d138a9a15715f00b085711c9d334b154a4e0c`, 187 × 270
pixels. `scripts/verify-cortez-lucero-portrait.py --archive PRIVATE_ARCHIVE`
checks the source capture receipt, original PDF digest, exact object stream,
asset bytes and image dimensions. The PDF's publication is dated 2017; the
photograph's creation date is unknown. This image establishes neither a 2021
vote nor an office term, which require separately sourced evidence. The initial
capture is private unreviewed discovery; this helper does not create any source
attestation or automatically publish a record.

For self-hosted publication, the private research archive must contain the
content-addressed PDF and JPEG and a digest-named, canonical source receipt.
The publisher verifies them before including the image; it withholds the image
if the proof or the motion is unavailable. The photo can be imported without
changing the research database. The six other voters do not yet have verified
portraits in this card.
