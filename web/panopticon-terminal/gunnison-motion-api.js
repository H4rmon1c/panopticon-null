const EXPECTED = {"schema_version":"pnull-gunnison-motion-v1","state":"available","research_record":"co-document-90325481a4958003979ad7f9","jurisdiction_ref":"us-co-county-08051","jurisdiction":"Gunnison County","meeting_date":"2026-09-15","body":"Board of County Commissioners","motion":"Approve a written notice of nonrenewal with Flock Group and authorize the chair’s signature","outcome":"Motion passed unanimously","motion_details":"Commissioner Smith moved to approve the notice and authorize the chair’s signature; Commissioner Houck seconded.","attendance":[{"name":"Laura Puckett Daniels","role":"Chairperson","individual_vote":"Not individually recorded"},{"name":"Elizabeth Smith","role":"Vice-Chairperson","individual_vote":"Not individually recorded"},{"name":"Jonathan Houck","role":"Commissioner","individual_vote":"Not individually recorded"}],"notice":"Item 10 was removed from the consent agenda and decided separately. Individual votes, a signed or delivered notice, actual camera removal, camera type, original contract approval and office terms are not established by this record.","source":{"url":"https://gunnisoncounty.org/AgendaCenter/ViewFile/Minutes/_09152026-1205","sha256":"79d3616ac6091c551f2be7d8f548c02becc0fbfff243e22ae664f231943a786f","text_sha256":"5480cfe5639a8a028f556569c93b0066eb059bbfbbdb58559d83f7da336a3b37","attendance_page":1,"motion_page":3,"attendance_locator":"extracted lines 1–9","motion_locator":"extracted lines 127–129 and 147–156","index_url":"https://gunnisoncounty.org/AgendaCenter","index_sha256":"0375ccd729eedb424009815981ad24d39c86b0530a2b1f4eb6f8a4809a6622c1","index_anchor":206,"index_receipt_sha256":"DYNAMIC","current_index_url":"DYNAMIC","current_index_sha256":"DYNAMIC","current_index_anchor":"DYNAMIC","current_index_receipt_sha256":"DYNAMIC","receipt_sha256":"DYNAMIC","captured_at":"DYNAMIC"}};
const canonical = x => JSON.stringify(x && typeof x === 'object' && !Array.isArray(x) ? Object.fromEntries(Object.keys(x).sort().map(k => [k,JSON.parse(canonical(x[k]))])) : Array.isArray(x) ? x.map(v=>JSON.parse(canonical(v))) : x);
export function validateGunnisonMotion(value) {
  if(value?.schema_version!==EXPECTED.schema_version)throw new Error('Invalid Gunnison sidecar');
  if(value.state==='not_available' && Object.keys(value).sort().join(',')==='schema_version,state')return value;
  const copy=structuredClone(value), source=copy?.source;
  for(const k of ['index_receipt_sha256','current_index_sha256','current_index_receipt_sha256','receipt_sha256']){
    if(typeof source?.[k]!=='string'||!/^[0-9a-f]{64}$/.test(source[k]))throw new Error('Invalid source proof');source[k]='DYNAMIC';
  }
  if(!Number.isSafeInteger(source.current_index_anchor)||source.current_index_anchor<0||source.current_index_anchor>100000)throw new Error('Invalid current index anchor');
  source.current_index_anchor='DYNAMIC';
  if(!['https://gunnisoncounty.org/AgendaCenter','https://gunnisoncounty.org/AgendaCenter#'].includes(source.current_index_url))throw new Error('Invalid current index URL');
  source.current_index_url='DYNAMIC';
  if(typeof source.captured_at!=='string'||!Number.isFinite(Date.parse(source.captured_at)))throw new Error('Invalid capture date');source.captured_at='DYNAMIC';
  if(canonical(copy)!==canonical(EXPECTED))throw new Error('Invalid Gunnison motion');
  return value;
}
export function validateGunnisonResearchBinding(value,research){
  if(value.state!=='available')return;
  const rows=research?.records?.filter(r=>r.id===value.research_record);
  if(!rows||rows.length!==1||rows[0].kind!=='meeting_document'||rows[0].historical!==false||rows[0].jurisdiction_ref!==value.jurisdiction_ref||rows[0].source_url!==value.source.url||rows[0].source_sha256!==value.source.sha256)throw new Error('Gunnison source changed');
}
