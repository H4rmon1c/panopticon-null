const EXPECTED = {"schema_version":"pnull-yuma-motion-v1","state":"available","research_record":"co-document-7f8320f04b37d2c99c52bcf4","jurisdiction_ref":"us-co-county-08125","jurisdiction":"Yuma County","meeting_date":"2025-06-30","body":"Board of County Commissioners","motion":"Approve a body-worn-camera contract with Axon","outcome":"Motion passed unanimously","attendance":[{"name":"Adam Gates","role":"Chairman","individual_vote":"Not individually recorded"},{"name":"Scott Weaver","role":"Commissioner","individual_vote":"Not individually recorded"},{"name":"Mike Leerar","role":"Commissioner (via phone)","individual_vote":"Not individually recorded"}],"notice":"The minutes are a summary, not a verbatim record. Individual votes, contract execution, delivery, deployment and office terms are not established by this record.","source":{"url":"https://yumacounty.net/wp-content/uploads/2025/08/Minutes2025.06.30_r.pdf","sha256":"3da13c5d09feb8704be698ec20228ff6370b49df61aa47e96d0ed16ea91c81a3","text_sha256":"ae8c0d74a145996a6f8baa2bd105ea67c823814238e005d8325d07aae852b7c9","attendance_page":1,"motion_page":2,"attendance_locator":"extracted lines 4–9","motion_locator":"extracted lines 65–74","index_url":"https://yumacounty.net/commissioners/","index_sha256":"e095b6a63ae6dd2e4e065dc1c500787d08ab767a82215169d02dbf9813eb7ca9","index_anchor":75,"index_receipt_sha256":"DYNAMIC","current_index_sha256":"DYNAMIC","current_index_anchor":"DYNAMIC","current_index_receipt_sha256":"DYNAMIC","receipt_sha256":"DYNAMIC","captured_at":"DYNAMIC"}};
const canonical = x => JSON.stringify(x && typeof x === 'object' && !Array.isArray(x) ? Object.fromEntries(Object.keys(x).sort().map(k => [k,JSON.parse(canonical(x[k]))])) : Array.isArray(x) ? x.map(v=>JSON.parse(canonical(v))) : x);
export function validateYumaMotion(value) {
  if(value?.schema_version!==EXPECTED.schema_version)throw new Error('Invalid Yuma sidecar');
  if(value.state==='not_available' && Object.keys(value).sort().join(',')==='schema_version,state')return value;
  const copy=structuredClone(value), source=copy?.source;
  for(const k of ['index_receipt_sha256','current_index_sha256','current_index_receipt_sha256','receipt_sha256']){
    if(typeof source?.[k]!=='string'||!/^[0-9a-f]{64}$/.test(source[k]))throw new Error('Invalid source proof');source[k]='DYNAMIC';
  }
  if(!Number.isSafeInteger(source.current_index_anchor)||source.current_index_anchor<0||source.current_index_anchor>100000)throw new Error('Invalid current index anchor');
  source.current_index_anchor='DYNAMIC';
  if(typeof source.captured_at!=='string'||!Number.isFinite(Date.parse(source.captured_at)))throw new Error('Invalid capture date');source.captured_at='DYNAMIC';
  if(canonical(copy)!==canonical(EXPECTED))throw new Error('Invalid Yuma motion');
  return value;
}
export function validateYumaResearchBinding(value,research){
  if(value.state!=='available')return;
  const rows=research?.records?.filter(r=>r.id===value.research_record);
  if(!rows||rows.length!==1||rows[0].kind!=='meeting_document'||rows[0].historical!==false||rows[0].jurisdiction_ref!==value.jurisdiction_ref||rows[0].source_url!==value.source.url||rows[0].source_sha256!==value.source.sha256)throw new Error('Yuma source changed');
}
