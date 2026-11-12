import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {validateGeorgetownMotion,validateGeorgetownResearchBinding} from '../georgetown-motion-api.js';
const data=JSON.parse(readFileSync(new URL('./fixtures/georgetown-motion-valid.json',import.meta.url)));
const record={id:data.research_record,kind:'meeting_document',historical:false,jurisdiction_ref:data.jurisdiction_ref,source_url:data.source.url,source_sha256:data.source.sha256,indexed_source_url:data.source.indexed_url,source_index_url:data.source.index_url,capture_adjustment:'timestamp_omitted'};
test('three ayes and three noes with a recorded police judge tie-break',()=>{
 validateGeorgetownMotion(data);validateGeorgetownResearchBinding(data,{records:[record]});assert.equal(data.votes.length,7);
 assert.equal(data.votes.filter(v=>v.position==='Aye').length,4);assert.equal(data.votes.filter(v=>v.position==='No').length,3);
 assert.deepEqual(data.votes.at(-1),{name:'Georgia Kofoed',role:'Police Judge (tie-break)',position:'Aye'});assert.match(data.notice,/did not approve or reject renewal/);
});
test('invented renewal, vote, tie-break or source fails closed',()=>{
 for(const edit of [d=>d.votes[0].position='No',d=>d.votes[6].role='Selectman',d=>d.votes[6].position='No',d=>d.motion='Approved Flock renewal',d=>d.source.roster_page=1,d=>d.source.motion_page=6,d=>d.source.sha256='0'.repeat(64),d=>d.source.indexed_document_receipt_sha256='invalid']){const x=structuredClone(data);edit(x);assert.throws(()=>validateGeorgetownMotion(x));}
});
test('missing changed or duplicated research source withholds',()=>{
 assert.throws(()=>validateGeorgetownResearchBinding(data,{records:[]}));assert.throws(()=>validateGeorgetownResearchBinding(data,{records:[record,record]}));
 for(const key of Object.keys(record))assert.throws(()=>validateGeorgetownResearchBinding(data,{records:[{...record,[key]:'changed'}]}));
});
test('browser labels tabling and withholds historical or missing record',async()=>{
 class Element{constructor(tag){this.tag=tag;this.textContent='';this.children=[];}append(...children){this.children.push(...children);}}
 const research=JSON.parse(readFileSync(new URL('../public/research-signals.json',import.meta.url)));
 async function render(sidecar,report,version){const status=new Element('status'),content=new Element('content');globalThis.document={querySelector:s=>s==='#motion-status'?status:content,createElement:t=>new Element(t)};globalThis.fetch=async url=>new Response(JSON.stringify(url.includes('georgetown-motion')?sidecar:report));await import(`../georgetown-motion.js?test=${version}`);return{status,content};}
 const yes=await render(data,research,'yes');assert.match(yes.status.textContent,/preliminary/);assert.equal(yes.content.children.find(x=>x.tag==='ul').children.length,7);assert.ok(yes.content.children.some(x=>x.textContent.includes('tabled renewal')));
 const no=await render({schema_version:data.schema_version,state:'not_available'},research,'no');assert.match(no.status.textContent,/unavailable/);assert.equal(no.content.children.length,0);
 const old=structuredClone(research);old.records.find(x=>x.id===data.research_record).historical=true;const hidden=await render(data,old,'old');assert.match(hidden.status.textContent,/unavailable/);assert.equal(hidden.content.children.length,0);
});
