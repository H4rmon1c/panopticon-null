const EXPECTED = {"schema_version":"pnull-hudson-motion-v1","state":"available","research_record":"co-document-84c83a2e5166b057153d3d7d","jurisdiction_ref":"us-co-place-0837820","jurisdiction":"Hudson","meeting_date":"2026-09-16","body":"Town Council","motion":"Approve Resolution 26-51 directing nonrenewal of Flock automatic license-plate-detection services","outcome":"Motion carried; six recorded ayes, no nays","motion_details":"Councilmember Matt Cole moved; Councilmember Rachel Thwaites seconded.","votes":[{"name":"Joe Hammock","role":"Mayor","position":"Aye"},{"name":"Don Post","role":"Mayor Pro Tem","position":"Aye"},{"name":"Matt Cole","role":"Councilmember","position":"Aye"},{"name":"Candace Nolf","role":"Councilmember","position":"Aye"},{"name":"Lisa Buesgens","role":"Councilmember","position":"Aye"},{"name":"Rachel Thwaites","role":"Councilmember","position":"Aye"}],"unrecorded_vote":"Councilmember Zach Reyes was present at roll call; his vote on this motion is not recorded in these minutes.","notice":"This was a nonrenewal decision, not approval of the original contract. Delivery of the notice, actual termination or removal of equipment, current service status and office terms are not established by this record.","source":{"url":"https://hudsonco.gov/AgendaCenter/ViewFile/Minutes/_09162026-780","sha256":"7cdad6608a7efa105bec154ee97487046b274f731a070e858bfffb554b185ef7","text_sha256":"e1c15a02b078b5b4f5bc6d86dfe8d6c410f3567818b4efcfbb31dd852c2a6953","roster_page":1,"motion_page":4,"attendance_locator":"OCR page 1","motion_locator":"OCR page 4","index_url":"https://hudsonco.gov/AgendaCenter","index_sha256":"3af10c7da90422d799051423e424509b3fa38d320a188b6bb77ba5e5a7aa3b5e","index_anchor":107,"index_receipt_sha256":"DYNAMIC","current_index_sha256":"DYNAMIC","current_index_anchor":"DYNAMIC","current_index_receipt_sha256":"DYNAMIC","receipt_sha256":"DYNAMIC","captured_at":"DYNAMIC"}};
const canonical = x => JSON.stringify(x && typeof x === 'object' && !Array.isArray(x) ? Object.fromEntries(Object.keys(x).sort().map(k => [k,JSON.parse(canonical(x[k]))])) : Array.isArray(x) ? x.map(v=>JSON.parse(canonical(v))) : x);
export function validateHudsonMotion(value) {
  if(value?.schema_version!==EXPECTED.schema_version)throw new Error('Invalid Hudson sidecar');
  if(value.state==='not_available' && Object.keys(value).sort().join(',')==='schema_version,state')return value;
  const copy=structuredClone(value), source=copy?.source;
  for(const k of ['index_receipt_sha256','current_index_sha256','current_index_receipt_sha256','receipt_sha256']){
    if(typeof source?.[k]!=='string'||!/^[0-9a-f]{64}$/.test(source[k]))throw new Error('Invalid source proof');source[k]='DYNAMIC';
  }
  if(!Number.isSafeInteger(source.current_index_anchor)||source.current_index_anchor<0||source.current_index_anchor>100000)throw new Error('Invalid current index anchor');
  source.current_index_anchor='DYNAMIC';
  if(typeof source.captured_at!=='string'||!Number.isFinite(Date.parse(source.captured_at)))throw new Error('Invalid capture date');source.captured_at='DYNAMIC';
  if(canonical(copy)!==canonical(EXPECTED))throw new Error('Invalid Hudson motion');
  return value;
}
export function validateHudsonResearchBinding(value,research){
  if(value.state!=='available')return;
  const rows=research?.records?.filter(r=>r.id===value.research_record);
  if(!rows||rows.length!==1||rows[0].kind!=='meeting_document'||rows[0].historical!==false||rows[0].jurisdiction_ref!==value.jurisdiction_ref||rows[0].source_url!==value.source.url||rows[0].source_sha256!==value.source.sha256)throw new Error('Hudson source changed');
}
