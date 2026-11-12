import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {validateColumbineMotion,validateColumbineResearchBinding} from '../columbine-motion-api.js';
const data=JSON.parse(readFileSync(new URL('./fixtures/columbine-motion-valid.json',import.meta.url)));
const record={id:data.research_record,kind:'meeting_document',historical:false,jurisdiction_ref:data.jurisdiction_ref,source_url:data.source.url,source_sha256:data.source.sha256};
test('unanimous outcome with attendance and no individual votes',()=>{
 validateColumbineMotion(data);validateColumbineResearchBinding(data,{records:[record]});assert.equal(data.outcome,'Motion passed unanimously');
 assert.equal(data.attendance.length,6);assert.ok(data.attendance.every(x=>x.individual_vote==='Not individually recorded'));assert.equal(data.votes,undefined);
 validateColumbineMotion({schema_version:data.schema_version,state:'not_available'});
});
test('invented vote, motion, identity, locator and source changes fail',()=>{
 const edits=[d=>d.attendance[0].individual_vote='Yes',d=>d.attendance[1].name='Other',d=>d.votes=[],d=>d.outcome='Motion failed',d=>d.motion='Camera purchase executed',d=>d.source.motion_page=1,d=>d.source.index_anchor=136,d=>d.source.sha256='0'.repeat(64),d=>d.source.current_index_anchor=-1,d=>d.source.current_index_receipt_sha256='invalid'];
 for(const edit of edits){const copy=structuredClone(data);edit(copy);assert.throws(()=>validateColumbineMotion(copy));}
});
test('missing historical or mismatched research records withhold',()=>{
 assert.throws(()=>validateColumbineResearchBinding(data,{records:[]}));assert.throws(()=>validateColumbineResearchBinding(data,{records:[record,record]}));
 for(const key of Object.keys(record))assert.throws(()=>validateColumbineResearchBinding(data,{records:[{...record,[key]:'changed'}]}));
});
test('browser shows historical date discrepancy and withholds changed source',async()=>{
 class Element {constructor(tag){this.tag=tag;this.textContent='';this.children=[];}append(...children){this.children.push(...children);}}
 const research=JSON.parse(readFileSync(new URL('../public/research-signals.json',import.meta.url)));
 async function render(sidecar,report,version){const status=new Element('status'),content=new Element('content');globalThis.document={querySelector:s=>s==='#motion-status'?status:content,createElement:tag=>new Element(tag)};globalThis.fetch=async url=>new Response(JSON.stringify(url.includes('columbine-motion')?sidecar:report));await import(`../columbine-motion.js?test=${version}`);return {status,content};}
 const shown=await render(data,research,'available');assert.match(shown.status.textContent,/preliminary/);
 assert.ok(shown.content.children.some(x=>x.textContent.includes('index labeled this file May 10')));
 assert.ok(shown.content.children.some(x=>x.textContent.includes('minutes themselves are dated May 20')));
 assert.equal(shown.content.children.find(x=>x.tag==='ul').children.length,6);
 const changed=structuredClone(research);changed.records.find(r=>r.id===data.research_record).historical=true;
 const withheld=await render(data,changed,'withdrawn');assert.match(withheld.status.textContent,/unavailable/);assert.equal(withheld.content.children.length,0);
});
