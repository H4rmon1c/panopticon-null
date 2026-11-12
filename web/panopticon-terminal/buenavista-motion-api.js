const EXPECTED = {"schema_version":"pnull-buenavista-motion-v1","state":"available","research_record":"co-document-70b769207d0e0c7d13953611","jurisdiction_ref":"us-co-place-0810105","jurisdiction":"Buena Vista","meeting_date":"2025-01-28","body":"Board of Trustees","motion":"Direct staff to evaluate a Chaffee County partnership concerning Flock cameras and return an MOU for consideration","outcome":"Carried; four AYES, two NYES","motion_details":"Trustee Swisher moved; Trustee Huyck seconded.","votes":[{"name":"Maggie Huyck","role":"Trustee","position":"Aye"},{"name":"Andrew Rice","role":"Trustee","position":"Aye"},{"name":"Chris Sturm","role":"Trustee","position":"Aye"},{"name":"Cindie Swisher","role":"Trustee","position":"Aye"},{"name":"Micha Rosenoer","role":"Trustee","position":"NYES (interpreted as no; unconfirmed)"},{"name":"Devin Rowe","role":"Trustee","position":"NYES (interpreted as no; unconfirmed)"}],"mayor":"Mayor Libby Fay was present; the motion’s roll-call table does not record a vote for her.","notice":"This motion directs staff to evaluate a partnership; it does not approve or establish an executed agreement, camera purchase or deployment. The source table prints NYES for two trustees; this is interpreted as no, but the source does not define the label.","source":{"url":"https://www.buenavistaco.gov/AgendaCenter/ViewFile/Minutes/_01282025-1883","sha256":"4af96edb75e01a5447b372128f379ad14416e7a23ec8a7b36f3f91096364353c","text_sha256":"48f4e437621dbec0089da4a5347e9097637b8862975b3b1b47d9a9f83dfc6174","roster_page":2,"appointment_page":4,"context_page":6,"motion_page":7,"attendance_locator":"OCR page 2","appointment_locator":"OCR page 4","motion_locator":"OCR page 7","index_url":"https://www.buenavistaco.gov/AgendaCenter/UpdateCategoryList?year=2025&catID=1&startDate=&endDate=&term=&prevVersionScreen=false","index_sha256":"48c9e9d2f6049456ea4a0f27e383f60e57bb59b182d98ed6b6364970abfc6cb1","index_anchor":76,"index_receipt_sha256":"DYNAMIC","current_index_sha256":"DYNAMIC","current_index_anchor":"DYNAMIC","current_index_receipt_sha256":"DYNAMIC","receipt_sha256":"DYNAMIC","captured_at":"DYNAMIC"}};
const canonical = x => JSON.stringify(x && typeof x === 'object' && !Array.isArray(x) ? Object.fromEntries(Object.keys(x).sort().map(k => [k,JSON.parse(canonical(x[k]))])) : Array.isArray(x) ? x.map(v=>JSON.parse(canonical(v))) : x);
export function validateBuenaVistaMotion(value) {
  if(value?.schema_version!==EXPECTED.schema_version)throw new Error('Invalid BuenaVista sidecar');
  if(value.state==='not_available' && Object.keys(value).sort().join(',')==='schema_version,state')return value;
  const copy=structuredClone(value), source=copy?.source;
  for(const k of ['index_receipt_sha256','current_index_sha256','current_index_receipt_sha256','receipt_sha256']){
    if(typeof source?.[k]!=='string'||!/^[0-9a-f]{64}$/.test(source[k]))throw new Error('Invalid source proof');source[k]='DYNAMIC';
  }
  if(!Number.isSafeInteger(source.current_index_anchor)||source.current_index_anchor<0||source.current_index_anchor>100000)throw new Error('Invalid current index anchor');
  source.current_index_anchor='DYNAMIC';
  if(typeof source.captured_at!=='string'||!Number.isFinite(Date.parse(source.captured_at)))throw new Error('Invalid capture date');source.captured_at='DYNAMIC';
  if(canonical(copy)!==canonical(EXPECTED))throw new Error('Invalid BuenaVista motion');
  return value;
}
export function validateBuenaVistaResearchBinding(value,research){
  if(value.state!=='available')return;
  const rows=research?.records?.filter(r=>r.id===value.research_record);
  if(!rows||rows.length!==1||rows[0].kind!=='meeting_document'||rows[0].historical!==false||rows[0].jurisdiction_ref!==value.jurisdiction_ref||rows[0].source_url!==value.source.url||rows[0].source_sha256!==value.source.sha256)throw new Error('BuenaVista source changed');
}
