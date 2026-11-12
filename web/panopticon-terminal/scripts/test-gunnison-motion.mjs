import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {validateGunnisonMotion,validateGunnisonResearchBinding} from '../gunnison-motion-api.js';
const data=JSON.parse(readFileSync(new URL('./fixtures/gunnison-motion-valid.json',import.meta.url)));
const record={id:data.research_record,kind:'meeting_document',historical:false,jurisdiction_ref:data.jurisdiction_ref,source_url:data.source.url,source_sha256:data.source.sha256};
test('unanimous outcome with attendance and no individual votes',()=>{
 validateGunnisonMotion(data);validateGunnisonResearchBinding(data,{records:[record]});assert.equal(data.outcome,'Motion passed unanimously');
 assert.equal(data.attendance.length,3);assert.ok(data.attendance.every(x=>x.individual_vote==='Not individually recorded'));assert.equal(data.votes,undefined);
 validateGunnisonMotion({schema_version:data.schema_version,state:'not_available'});
});
test('invented vote, motion, identity, locator and source changes fail',()=>{
 const edits=[d=>d.attendance[0].individual_vote='Yes',d=>d.attendance[1].name='Other',d=>d.votes=[],d=>d.outcome='Motion failed',d=>d.motion='Original Flock agreement approved',d=>d.source.motion_page=1,d=>d.source.index_anchor=207,d=>d.source.sha256='0'.repeat(64),d=>d.source.current_index_anchor=-1,d=>d.source.current_index_receipt_sha256='invalid',d=>d.source.current_index_url='https://evil.example/'];
 for(const edit of edits){const copy=structuredClone(data);edit(copy);assert.throws(()=>validateGunnisonMotion(copy));}
});
test('missing historical or mismatched research records withhold',()=>{
 assert.throws(()=>validateGunnisonResearchBinding(data,{records:[]}));assert.throws(()=>validateGunnisonResearchBinding(data,{records:[record,record]}));
 for(const key of Object.keys(record))assert.throws(()=>validateGunnisonResearchBinding(data,{records:[{...record,[key]:'changed'}]}));
});
test('browser displays nonrenewal and withholds on changed or withdrawn evidence',async()=>{
 class Element {constructor(tag){this.tag=tag;this.textContent='';this.children=[];}append(...children){this.children.push(...children);}}
 const research=JSON.parse(readFileSync(new URL('../public/research-signals.json',import.meta.url)));
 async function render(sidecar,report,version){
  const status=new Element('status'),content=new Element('content');
  globalThis.document={querySelector:s=>s==='#motion-status'?status:content,createElement:tag=>new Element(tag)};
  globalThis.fetch=async url=>new Response(JSON.stringify(url.includes('gunnison-motion')?sidecar:report));
  await import(`../gunnison-motion.js?test=${version}`);return {status,content};
 }
 const shown=await render(data,research,'available');assert.match(shown.status.textContent,/preliminary/);
 assert.ok(shown.content.children.some(x=>x.textContent.includes('nonrenewal')));
 assert.ok(shown.content.children.some(x=>x.textContent.includes('removed from the consent agenda')));
 assert.equal(shown.content.children.find(x=>x.tag==='ul').children.length,3);
 const missing=await render({schema_version:data.schema_version,state:'not_available'},research,'missing');
 assert.match(missing.status.textContent,/unavailable/);assert.equal(missing.content.children.length,0);
 const changed=structuredClone(research);changed.records.find(r=>r.id===data.research_record).historical=true;
 const withdrawn=await render(data,changed,'withdrawn');assert.match(withdrawn.status.textContent,/unavailable/);assert.equal(withdrawn.content.children.length,0);
});
