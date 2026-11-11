# Del Norte document-index profile

The [Town of Del Norte minutes and agendas page](https://www.delnortecolorado.com/minutes-agendas/)
lists town board agendas and approved minutes by year. Its linked PDFs include
the [November 5, 2026 agenda](https://www.delnortecolorado.com/wp-content/uploads/2026/11/November-5-2026-Town-Board-Meeting-Agenda-ACC.pdf)
and the [May 14, 2025 minutes](https://www.delnortecolorado.com/wp-content/uploads/2025/09/May-14-2025-Town-Board-Meeting-Minutes-1ACC.pdf).
These links establish a plausible official document source, **not** a relevant
surveillance decision or an individual voting position. The May minutes use
phrases such as “all ayes”; the proposal builder does not turn that phrase or
an attendance list into named votes.

[`co-del-norte.toml`](../configs/states/co-del-norte.toml) configures the exact
index URL and a separate same-host `/wp-content/uploads/` PDF path policy.
The index maps to `us-co-place-0819795` in the neutral geography roster.
This profile remains **unscheduled and uncaptured**. Both source policies need
current robots/terms reviews before the private collector can fetch them.
The browser has no live dependency on the town website.

The document-index adapter now accepts a reviewed exact HTML index path and a
reviewed same-host PDF prefix instead of a fixed list of two index paths.
Candidates must be `.pdf` URLs beneath that prefix, with no query, fragment,
encoded path segment, repeated slash, or cross-host jump. The origin index,
its raw bytes, fetch receipt, link anchor, and document path policy are checked
again when a candidate enters a private proposal. Other publishing platforms
still need their own constrained source profiles and source reviews.
