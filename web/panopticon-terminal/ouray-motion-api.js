const EXPECTED = {"schema_version":"pnull-ouray-motion-v1","state":"available","research_record":"co-document-a715fed2f776dc7376814eb0","jurisdiction_ref":"us-co-county-08091","jurisdiction":"Ouray County","meeting_date":"2024-11-19","body":"Board of County Commissioners","motion":"Approve and authorize the chair’s signature on an Axon quotation and fiscal impact form for taser and camera software and services","outcome":"Motion passed unanimously","attendance":[{"name":"Michelle Nauer","role":"Chair","individual_vote":"Not individually recorded"},{"name":"Lynn Padgett","role":"Vice-Chair","individual_vote":"Not individually recorded"},{"name":"Jake Niece","role":"Commissioner","individual_vote":"Not individually recorded"}],"notice":"The motion authorized signatures. Individual votes, actual signatures, contract execution, delivery, deployment and office terms are not established by this record.","source":{"url":"https://ouraycountyco.gov/AgendaCenter/ViewFile/Minutes/_11192024-928","sha256":"b32eeec9d0467253693983f5f459675715a48f3d76be227239778a3f8a8bd10b","text_sha256":"c2ca5c366e9d6b31d249deedd9e165401570709c777f967f7bf27c0438553a40","attendance_page":1,"motion_page":2,"attendance_locator":"extracted lines 1–2","motion_locator":"extracted lines 127–132","index_url":"https://ouraycountyco.gov/AgendaCenter/UpdateCategoryList?year=2024&catID=1&startDate=&endDate=&term=&prevVersionScreen=false","index_sha256":"8789592383ffc62b8d50c010ce212996f9a88e953d380e913aa5712e83a9bf60","index_anchor":39,"index_receipt_sha256":"DYNAMIC","current_index_sha256":"DYNAMIC","current_index_anchor":"DYNAMIC","current_index_receipt_sha256":"DYNAMIC","receipt_sha256":"DYNAMIC","captured_at":"DYNAMIC"}};
const canonical = x => JSON.stringify(x && typeof x === 'object' && !Array.isArray(x) ? Object.fromEntries(Object.keys(x).sort().map(k => [k,JSON.parse(canonical(x[k]))])) : Array.isArray(x) ? x.map(v=>JSON.parse(canonical(v))) : x);
export function validateOurayMotion(value) {
  if(value?.schema_version!==EXPECTED.schema_version)throw new Error('Invalid Ouray sidecar');
  if(value.state==='not_available' && Object.keys(value).sort().join(',')==='schema_version,state')return value;
  const copy=structuredClone(value), source=copy?.source;
  for(const k of ['index_receipt_sha256','current_index_sha256','current_index_receipt_sha256','receipt_sha256']){
    if(typeof source?.[k]!=='string'||!/^[0-9a-f]{64}$/.test(source[k]))throw new Error('Invalid source proof');source[k]='DYNAMIC';
  }
  if(!Number.isSafeInteger(source.current_index_anchor)||source.current_index_anchor<0||source.current_index_anchor>100000)throw new Error('Invalid current index anchor');
  source.current_index_anchor='DYNAMIC';
  if(typeof source.captured_at!=='string'||!Number.isFinite(Date.parse(source.captured_at)))throw new Error('Invalid capture date');source.captured_at='DYNAMIC';
  if(canonical(copy)!==canonical(EXPECTED))throw new Error('Invalid Ouray motion');
  return value;
}
export function validateOurayResearchBinding(value,research){
  if(value.state!=='available')return;
  const rows=research?.records?.filter(r=>r.id===value.research_record);
  if(!rows||rows.length!==1||rows[0].kind!=='meeting_document'||rows[0].historical!==false||rows[0].jurisdiction_ref!==value.jurisdiction_ref||rows[0].source_url!==value.source.url||rows[0].source_sha256!==value.source.sha256)throw new Error('Ouray source changed');
}
