// Separate fixed source sidecar; the research-v1 schema remains unchanged.
const only = (value, keys) => value && typeof value === 'object' && !Array.isArray(value) &&
  Object.keys(value).length === keys.length && keys.every((key) => Object.hasOwn(value, key));
const votes = [
  ['Robert Dobry', 'Councilmember', 'Dobry'], ['Amy Huckins', 'Councilmember', 'Huckins'],
  ['Mike Lavey', 'Mayor', 'Lavey'], ['Orly Lucero', 'Councilmember', 'Lucero'],
  ['Rachel Medina', 'Mayor Pro Tem', 'Medina'], ['David Rainey', 'Councilmember', 'Rainey'],
  ['Arlina Yazzie', 'Councilmember', 'Yazzie'],
];
export function validateCortezMotion(data) {
  if (data?.schema_version !== 'pnull-cortez-motion-v1') throw new Error('Invalid Cortez sidecar');
  if (data.state === 'not_available' && only(data, ['schema_version','state'])) return data;
  if (!only(data,['schema_version','state','research_record','jurisdiction_ref','jurisdiction','meeting_date','body','motion','scope','outcome','votes','notice','source']) ||
      data.state !== 'available' || data.research_record !== 'co-document-86c05ac1faae3595c83d3344' ||
      data.jurisdiction_ref !== 'us-co-place-0817375' || data.jurisdiction !== 'Cortez' ||
      data.meeting_date !== '2021-03-23' || data.body !== 'City Council' ||
      data.motion !== 'Final reading of Ordinance 1286 approving purchase of items from Axon Enterprise, Inc.' ||
      data.scope !== 'Item-specific final-reading vote; separate from the consent agenda.' || data.outcome !== 'Seven yes votes' ||
      data.notice !== 'Minutes describe tasers, body cameras and interview-room equipment. Executed contract, delivery, deployment and office terms are not established by this record.' ||
      !Array.isArray(data.votes) || data.votes.length !== votes.length) throw new Error('Invalid Cortez identity');
  for (const [i,vote] of data.votes.entries()) {
    if (!only(vote,['name','role','table_label','position']) || vote.name !== votes[i][0] || vote.role !== votes[i][1] ||
        vote.table_label !== votes[i][2] || vote.position !== 'Yes') throw new Error('Invalid Cortez vote');
  }
  const s=data.source;
  if (!only(s,['url','sha256','text_sha256','roster_page','vote_page','roster_locator','motion_locator','vote_locator','index_url','index_sha256','index_anchor','receipt_sha256','index_receipt_sha256','captured_at']) ||
      s.url !== 'https://www.cortezco.gov/AgendaCenter/ViewFile/Minutes/_03232021-768' ||
      s.sha256 !== '9a9a750885280e7cad5226d573ace94713cfa50d547e26b55eba96aebb72d67a' ||
      s.text_sha256 !== 'f1c88cdb6bfc865458f9e627f3c1c175efce24deefaf82fa99dd54bf0b32aa32' ||
      s.roster_page !== 1 || s.vote_page !== 2 || s.roster_locator !== 'extracted lines 1–8' ||
      s.motion_locator !== 'extracted lines 70–80' || s.vote_locator !== 'extracted lines 81–95; visual table on page 2' ||
      s.index_url !== 'https://www.cortezco.gov/AgendaCenter/UpdateCategoryList?year=2021&catID=3&startDate=&endDate=&term=&prevVersionScreen=false' ||
      s.index_sha256 !== '305f5dfe53f80c0fe37951fa2224b045affc532e77030e97f073c06e10c5c60e' || s.index_anchor !== 69 ||
      !/^[0-9a-f]{64}$/.test(s.receipt_sha256) || !/^[0-9a-f]{64}$/.test(s.index_receipt_sha256) ||
      typeof s.captured_at !== 'string' || !Number.isFinite(Date.parse(s.captured_at))) throw new Error('Invalid Cortez source');
  return data;
}
export function validateCortezResearchBinding(data,research) {
  if (data.state !== 'available') return;
  const records=research?.records?.filter((row) => row.id === data.research_record);
  if (!records || records.length !== 1 || records[0].kind !== 'meeting_document' || records[0].historical !== false ||
      records[0].jurisdiction_ref !== data.jurisdiction_ref || records[0].source_url !== data.source.url ||
      records[0].source_sha256 !== data.source.sha256) throw new Error('Cortez motion and source do not match');
}
