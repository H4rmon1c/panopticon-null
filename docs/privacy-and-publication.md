# Privacy and publication

Public availability does not automatically justify republication.

## Non-negotiable boundaries

- Raw sensitive logs and original records remain local.
- Public output contains only information necessary to establish institutional conduct.
- Plate numbers, home addresses, private identities, movement histories, personal contact details, and similarly sensitive identifiers must not appear.
- Every free-text field is potentially sensitive.
- The project monitors institutions, procurements, contracts, policies, and public decisions—not private citizens.
- No dossiers on activists, officers, employees, residents, or other people.
- No facial recognition and no person-level movement analysis.
- No harassment, doxxing, unauthorized access, or physical interference.

The static site publishes selected citations, not complete extracted records. Meaningful diffs include only cited changed lines. Local evidence and SQLite directories are mode `0700` on Unix; record and database files are mode `0600` where set directly.

Automated checks reject recognized plate labels, email addresses, common street-address forms, Social Security numbers, personal contact/location field labels, coordinates, and movement logs. Pattern matching cannot detect every sensitive value. Before distributing generated files or approving an X draft, a human must inspect every citation and diff. If relevance is uncertain, do not publish.

The purpose is to constrain the panopticon, not recreate it with different operators.
