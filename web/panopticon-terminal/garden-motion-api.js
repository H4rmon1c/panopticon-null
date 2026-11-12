// This separate, versioned sidecar does not alter the strict research-v1 schema.
const only = (value, keys) => value && typeof value === 'object' && !Array.isArray(value) &&
  Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
const votes = [
  ['Fil Archuleta', 'Mayor', 'extracted line 210'], ['Alex Lopez', 'Trustee', 'extracted line 210'],
  ['Leigh Sorensen', 'Trustee', 'extracted line 210'], ['Katherine Rodriguez', 'Trustee', 'extracted line 211'],
  ['Gary Sorensen', 'Trustee', 'extracted line 211'], ['Jasmine Marquez', 'Trustee', 'extracted line 211'],
];
export function validateGardenMotion(data) {
  if (data?.schema_version !== 'pnull-garden-city-motion-v1') throw new Error('Invalid motion sidecar');
  if (data.state === 'not_available' && only(data, ['schema_version', 'state'])) return data;
  if (!only(data, ['schema_version', 'state', 'research_record', 'jurisdiction_ref', 'jurisdiction', 'meeting_date', 'body', 'motion', 'outcome', 'votes', 'attendance', 'minutes_adoption', 'source', 'notice']) ||
      data.state !== 'available' || data.research_record !== 'co-document-65cfe2b3b5d87f9ab45c4235' ||
      data.jurisdiction_ref !== 'us-co-place-0829185' || data.jurisdiction !== 'Garden City' ||
      data.meeting_date !== '2026-08-03' || data.body !== 'Board of Trustees' ||
      data.motion !== 'Motion to approve a five-year agreement with Axon Enterprise, Inc.' || data.outcome !== 'Motion carried' ||
      data.notice !== 'Minutes included in a later board packet. Contract execution and a specific technology are not established by this record.' ||
      !Array.isArray(data.votes) || data.votes.length !== votes.length) throw new Error('Invalid motion identity');
  for (const [i, vote] of data.votes.entries()) {
    if (!only(vote, ['name', 'role', 'position', 'locator']) || vote.name !== votes[i][0] || vote.role !== votes[i][1] ||
        vote.position !== 'Yes' || vote.locator !== votes[i][2]) throw new Error('Invalid motion vote');
  }
  const a = data.attendance;
  if (!only(a, ['name', 'role', 'status', 'locator']) || a.name !== 'Rebecca Moreau' || a.role !== 'Trustee' ||
      a.status !== 'Absent at roll call' || a.locator !== 'extracted line 87') throw new Error('Invalid attendance context');
  const s = data.source;
  if (!only(s, ['url', 'sha256', 'meeting_page', 'motion_page', 'text_sha256', 'meeting_locator', 'motion_locator', 'index_url', 'index_sha256', 'index_anchor', 'captured_at', 'receipt_sha256', 'index_receipt_sha256']) ||
      s.url !== 'https://www.townofgardencity.com/files/415f86adc/Board+Packet+8.18.26+Amended.pdf' ||
      s.sha256 !== 'a6470690665a246b48a279bccde538eb3c1b6a108793bee55dae249df981c5c7' || s.meeting_page !== 3 || s.motion_page !== 6 ||
      s.text_sha256 !== 'becbea24e7b1d83b25644c4c3a55381fe52cd1d9a8837f898aeeecea5f0ac9c5' ||
      s.meeting_locator !== 'extracted lines 70–71' || s.motion_locator !== 'extracted lines 203–213' ||
      s.index_url !== 'https://www.townofgardencity.com/board-meetings' ||
      s.index_sha256 !== 'b3d4db4c22ef8241b8a6a4cea0567ea8293f26b309bfd07411ebc7fa9d31fc76' || s.index_anchor !== 90 ||
      !/^[0-9a-f]{64}$/.test(s.receipt_sha256) || !/^[0-9a-f]{64}$/.test(s.index_receipt_sha256) ||
      typeof s.captured_at !== 'string' || !Number.isFinite(Date.parse(s.captured_at))) throw new Error('Invalid motion source');
  const adoption = data.minutes_adoption;
  if (adoption !== null) {
    if (!only(adoption, ['date', 'scope', 'outcome', 'yes_count', 'notice', 'source']) ||
        adoption.date !== '2026-08-18' || adoption.scope !== 'Consent agenda a–g, including approval of the August 3 minutes' ||
        adoption.outcome !== 'Motion carried' || adoption.yes_count !== 7 ||
        adoption.notice !== 'This later consent-block vote approved minutes and other items; it is not a vote on the Axon agreement.') throw new Error('Invalid adoption context');
    const t = adoption.source;
    if (!only(t, ['url', 'sha256', 'text_sha256', 'meeting_page', 'vote_page', 'locator', 'index_url', 'index_sha256', 'index_anchor', 'receipt_sha256', 'index_receipt_sha256', 'captured_at']) ||
        t.url !== 'https://www.townofgardencity.com/files/a5a4e0d2c/Minutes+8.18.26.pdf' ||
        t.sha256 !== '1fadbfb3ebfc9409c6cb6c6088203048342fe3d90c9db0f91291b34077abf07e' ||
        t.text_sha256 !== 'c39514d6a75f1d9d3a89e9c973e563c7066bfd281e35f349a4d54a628e95d3f5' ||
        t.meeting_page !== 1 || t.vote_page !== 2 || t.locator !== 'extracted lines 22–44' ||
        t.index_url !== 'https://www.townofgardencity.com/board-meetings' ||
        t.index_sha256 !== 'ba10f90f69057113bfa08ea546d7daa559c159837cc9f6efbf6013bb3e6d1485' || t.index_anchor !== 89 ||
        !/^[0-9a-f]{64}$/.test(t.receipt_sha256) || !/^[0-9a-f]{64}$/.test(t.index_receipt_sha256) ||
        typeof t.captured_at !== 'string' || !Number.isFinite(Date.parse(t.captured_at))) throw new Error('Invalid adoption source');
  }
  return data;
}

// Pairing is checked at publication and in the browser against the research snapshot.
export function validateGardenResearchBinding(data, research) {
  if (data.state !== 'available') return;
  const records = research?.records?.filter((r) => r.id === data.research_record);
  if (!records || records.length !== 1 || records[0].kind !== 'meeting_document' || records[0].historical !== false || records[0].jurisdiction_ref !== data.jurisdiction_ref ||
      records[0].source_url !== data.source.url || records[0].source_sha256 !== data.source.sha256)
    throw new Error('Motion and research source do not match');
}
