const EXPECTED = {"schema_version":"pnull-clearcreek-motion-v1","state":"available","research_record":"co-document-7449fbbc2433c23d2da0f8ed","jurisdiction_ref":"us-co-county-08019","jurisdiction":"Clear Creek County","meeting_date":"2025-12-16","body":"Board of County Commissioners","motion":"Consent agenda including an Axon quote and master services agreement for body-worn cameras, in-car video, TASER devices, digital evidence storage and software licensing","outcome":"Consent agenda passed unanimously","block_motion":"Commissioner Hartman-Ball moved to approve the consent agenda; Commissioner Lloyd seconded.","attendance":[{"name":"George Marlin","role":"Chair","individual_vote":"Not individually recorded"},{"name":"Jodie Hartman Ball","role":"Commissioner","individual_vote":"Not individually recorded"},{"name":"Rebecca Lloyd","role":"Commissioner","individual_vote":"Not individually recorded"}],"notice":"The recorded motion covers the entire consent agenda, including the Axon item. A separate item vote, individual roll-call votes, contract execution, delivery, deployment and office terms are not established by this record.","source":{"url":"https://www.clearcreekcounty.us/AgendaCenter/ViewFile/Minutes/_12162025-901","sha256":"3029fdaa63306a107060a576a155347e2aee2e93f8aef4d9a347988275d2100c","text_sha256":"a24269216b3fc283d8f9b9b3e2eb6449f4678a95d6ae78ada406cd590036f7a8","attendance_page":1,"motion_page":2,"attendance_locator":"extracted lines 1\u20139","motion_locator":"extracted lines 38\u201357","index_url":"https://www.clearcreekcounty.us/AgendaCenter/UpdateCategoryList?year=2025&catID=4&startDate=&endDate=&term=&prevVersionScreen=false","index_sha256":"9d6cd24774e3c9d49e70f2cfc16e79a1ed21cfa93627ef2c268fd843fcc029e3","index_anchor":9,"index_receipt_sha256":"DYNAMIC","current_index_sha256":"DYNAMIC","current_index_anchor":"DYNAMIC","current_index_receipt_sha256":"DYNAMIC","receipt_sha256":"DYNAMIC","captured_at":"DYNAMIC"}};
const canonical = x => JSON.stringify(x && typeof x === 'object' && !Array.isArray(x) ? Object.fromEntries(Object.keys(x).sort().map(k => [k,JSON.parse(canonical(x[k]))])) : Array.isArray(x) ? x.map(v=>JSON.parse(canonical(v))) : x);
export function validateClearCreekMotion(value) {
  if(value?.schema_version!==EXPECTED.schema_version)throw new Error('Invalid ClearCreek sidecar');
  if(value.state==='not_available' && Object.keys(value).sort().join(',')==='schema_version,state')return value;
  const copy=structuredClone(value), source=copy?.source;
  for(const k of ['index_receipt_sha256','current_index_sha256','current_index_receipt_sha256','receipt_sha256']){
    if(typeof source?.[k]!=='string'||!/^[0-9a-f]{64}$/.test(source[k]))throw new Error('Invalid source proof');source[k]='DYNAMIC';
  }
  if(!Number.isSafeInteger(source.current_index_anchor)||source.current_index_anchor<0||source.current_index_anchor>100000)throw new Error('Invalid current index anchor');
  source.current_index_anchor='DYNAMIC';
  if(typeof source.captured_at!=='string'||!Number.isFinite(Date.parse(source.captured_at)))throw new Error('Invalid capture date');source.captured_at='DYNAMIC';
  if(canonical(copy)!==canonical(EXPECTED))throw new Error('Invalid ClearCreek motion');
  return value;
}
export function validateClearCreekResearchBinding(value,research){
  if(value.state!=='available')return;
  const rows=research?.records?.filter(r=>r.id===value.research_record);
  if(!rows||rows.length!==1||rows[0].kind!=='meeting_document'||rows[0].historical!==false||rows[0].jurisdiction_ref!==value.jurisdiction_ref||rows[0].source_url!==value.source.url||rows[0].source_sha256!==value.source.sha256)throw new Error('ClearCreek source changed');
}
