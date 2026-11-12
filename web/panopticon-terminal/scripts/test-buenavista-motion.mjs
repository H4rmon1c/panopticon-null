import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {validateBuenaVistaMotion,validateBuenaVistaResearchBinding} from '../buenavista-motion-api.js';
const data=JSON.parse(readFileSync(new URL('./fixtures/buenavista-motion-valid.json',import.meta.url)));
const record={id:data.research_record,kind:'meeting_document',historical:false,jurisdiction_ref:data.jurisdiction_ref,source_url:data.source.url,source_sha256:data.source.sha256};
test('four AYES and two NYES on staff evaluation, no mayor vote',()=>{
 validateBuenaVistaMotion(data);validateBuenaVistaResearchBinding(data,{records:[record]});assert.equal(data.votes.length,6);
 assert.equal(data.votes.filter(v=>v.position==='Aye').length,4);assert.equal(data.votes.filter(v=>v.position.includes('NYES')).length,2);assert.ok(data.votes.every(v=>v.name!=='Libby Fay'));assert.match(data.notice,/does not approve/);
});
test('invented approval, altered vote, appointee, source or locator fail closed',()=>{
 for(const edit of [d=>d.votes[0].position='No',d=>d.votes[2].name='Other',d=>d.motion='Approved Flock contract',d=>d.mayor='Mayor voted yes',d=>d.source.appointment_page=2,d=>d.source.motion_page=6,d=>d.source.sha256='0'.repeat(64),d=>d.source.current_index_receipt_sha256='invalid']){const x=structuredClone(data);edit(x);assert.throws(()=>validateBuenaVistaMotion(x));}
});
test('missing changed or duplicated research source withholds',()=>{
 assert.throws(()=>validateBuenaVistaResearchBinding(data,{records:[]}));assert.throws(()=>validateBuenaVistaResearchBinding(data,{records:[record,record]}));
 for(const key of Object.keys(record))assert.throws(()=>validateBuenaVistaResearchBinding(data,{records:[{...record,[key]:'changed'}]}));
});
test('browser labels evaluation and withholds historical or missing record',async()=>{
 class Element{constructor(tag){this.tag=tag;this.textContent='';this.children=[];}append(...children){this.children.push(...children);}}
 const research=JSON.parse(readFileSync(new URL('../public/research-signals.json',import.meta.url)));
 async function render(sidecar,report,version){const status=new Element('status'),content=new Element('content');globalThis.document={querySelector:s=>s==='#motion-status'?status:content,createElement:t=>new Element(t)};globalThis.fetch=async url=>new Response(JSON.stringify(url.includes('buenavista-motion')?sidecar:report));await import(`../buenavista-motion.js?test=${version}`);return{status,content};}
 const yes=await render(data,research,'yes');assert.match(yes.status.textContent,/preliminary/);assert.equal(yes.content.children.find(x=>x.tag==='ul').children.length,6);assert.ok(yes.content.children.some(x=>x.textContent.includes('does not approve')));
 const no=await render({schema_version:data.schema_version,state:'not_available'},research,'no');assert.match(no.status.textContent,/unavailable/);assert.equal(no.content.children.length,0);
 const old=structuredClone(research);old.records.find(x=>x.id===data.research_record).historical=true;const hidden=await render(data,old,'old');assert.match(hidden.status.textContent,/unavailable/);assert.equal(hidden.content.children.length,0);
});
