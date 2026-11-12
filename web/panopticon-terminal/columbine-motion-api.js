const EXPECTED = {"schema_version":"pnull-columbine-motion-v1","state":"available","research_record":"co-document-cf6d9e8394298e3b40d75489","jurisdiction_ref":"us-co-place-0816385","jurisdiction":"Columbine Valley","meeting_date":"2025-05-20","body":"Board of Trustees","motion":"Approve a Littleton license agreement described as allowing the town to place a Flock camera in Littleton right-of-way","outcome":"Motion passed unanimously","motion_details":"Trustee Icenogle moved to approve the agreement; Trustee Giesen seconded.","attendance":[{"name":"Bill Dotson","role":"Mayor Pro Tem","individual_vote":"Not individually recorded"},{"name":"Mike Giesen","role":"Trustee","individual_vote":"Not individually recorded"},{"name":"Dave Huelskamp","role":"Trustee","individual_vote":"Not individually recorded"},{"name":"Ed Icenogle","role":"Trustee","individual_vote":"Not individually recorded"},{"name":"Betsy McCain","role":"Trustee","individual_vote":"Not individually recorded"},{"name":"Al Timothy","role":"Trustee (participated virtually)","individual_vote":"Not individually recorded"}],"notice":"The archived town index labeled this file May 10; the minutes themselves are dated May 20. Individual votes, the executed agreement, camera purchase, installation, camera type, current use and office terms are not established by this record.","source":{"url":"https://columbinevalley.org/wp-content/uploads/2025/06/05-20-25-Board-of-Trustee-Minutes.pdf","sha256":"4616214c06fe1a10fc859c987a4ff5ec83091fb09aae3e23c6c5bacc85841c48","text_sha256":"67f5f0e649eea711ad08794b89a2ae0e75dc1008beba4e11e7c88c4d8f85df7b","attendance_page":1,"motion_page":2,"attendance_locator":"extracted lines 1–14","motion_locator":"extracted lines 45–49","index_url":"https://columbinevalley.org/board-trustee-agendas-minutes/","index_sha256":"cf4629ca406d3ffc660c188a57a19c63f3051eabdd087177de2e99b13b8a0896","index_anchor":135,"index_receipt_sha256":"DYNAMIC","current_index_sha256":"DYNAMIC","current_index_anchor":"DYNAMIC","current_index_receipt_sha256":"DYNAMIC","receipt_sha256":"DYNAMIC","captured_at":"DYNAMIC"}};
const canonical = x => JSON.stringify(x && typeof x === 'object' && !Array.isArray(x) ? Object.fromEntries(Object.keys(x).sort().map(k => [k,JSON.parse(canonical(x[k]))])) : Array.isArray(x) ? x.map(v=>JSON.parse(canonical(v))) : x);
export function validateColumbineMotion(value) {
  if(value?.schema_version!==EXPECTED.schema_version)throw new Error('Invalid Columbine sidecar');
  if(value.state==='not_available' && Object.keys(value).sort().join(',')==='schema_version,state')return value;
  const copy=structuredClone(value), source=copy?.source;
  for(const k of ['index_receipt_sha256','current_index_sha256','current_index_receipt_sha256','receipt_sha256']){
    if(typeof source?.[k]!=='string'||!/^[0-9a-f]{64}$/.test(source[k]))throw new Error('Invalid source proof');source[k]='DYNAMIC';
  }
  if(!Number.isSafeInteger(source.current_index_anchor)||source.current_index_anchor<0||source.current_index_anchor>100000)throw new Error('Invalid current index anchor');
  source.current_index_anchor='DYNAMIC';
  if(typeof source.captured_at!=='string'||!Number.isFinite(Date.parse(source.captured_at)))throw new Error('Invalid capture date');source.captured_at='DYNAMIC';
  if(canonical(copy)!==canonical(EXPECTED))throw new Error('Invalid Columbine motion');
  return value;
}
export function validateColumbineResearchBinding(value,research){
  if(value.state!=='available')return;
  const rows=research?.records?.filter(r=>r.id===value.research_record);
  if(!rows||rows.length!==1||rows[0].kind!=='meeting_document'||rows[0].historical!==false||rows[0].jurisdiction_ref!==value.jurisdiction_ref||rows[0].source_url!==value.source.url||rows[0].source_sha256!==value.source.sha256)throw new Error('Columbine source changed');
}
