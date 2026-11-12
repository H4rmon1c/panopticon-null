import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {validateClearCreekMotion,validateClearCreekResearchBinding} from '../clearcreek-motion-api.js';
const data=JSON.parse(readFileSync(new URL('./fixtures/clearcreek-motion-valid.json',import.meta.url)));
const record={id:data.research_record,kind:'meeting_document',historical:false,jurisdiction_ref:data.jurisdiction_ref,source_url:data.source.url,source_sha256:data.source.sha256};
test('unanimous outcome with attendance and no individual votes',()=>{
 validateClearCreekMotion(data);validateClearCreekResearchBinding(data,{records:[record]});assert.equal(data.outcome,'Consent agenda passed unanimously');
 assert.equal(data.attendance.length,3);assert.ok(data.attendance.every(x=>x.individual_vote==='Not individually recorded'));assert.equal(data.votes,undefined);
 validateClearCreekMotion({schema_version:data.schema_version,state:'not_available'});
});
test('invented vote, motion, identity, locator and source changes fail',()=>{
 const edits=[d=>d.attendance[0].individual_vote='Yes',d=>d.attendance[1].name='Other',d=>d.votes=[],d=>d.outcome='Motion failed',d=>d.block_motion='An individual voted yes',d=>d.notice='Separate item vote',d=>d.motion='Different contract',d=>d.source.motion_page=1,d=>d.source.index_anchor=10,d=>d.source.sha256='0'.repeat(64),d=>d.source.current_index_anchor=-1,d=>d.source.current_index_receipt_sha256='invalid'];
 for(const edit of edits){const copy=structuredClone(data);edit(copy);assert.throws(()=>validateClearCreekMotion(copy));}
});
test('missing historical or mismatched research records withhold',()=>{
 assert.throws(()=>validateClearCreekResearchBinding(data,{records:[]}));assert.throws(()=>validateClearCreekResearchBinding(data,{records:[record,record]}));
 for(const key of Object.keys(record))assert.throws(()=>validateClearCreekResearchBinding(data,{records:[{...record,[key]:'changed'}]}));
});

test('browser labels the consent block and withholds unavailable records',async()=>{
 class E{constructor(tag){this.tag=tag;this.children=[];this.textContent=''}append(...x){this.children.push(...x)}}
 const report=JSON.parse(readFileSync(new URL('../public/research-signals.json',import.meta.url)));
 async function render(value,version){const status=new E('status'),content=new E('content');globalThis.document={querySelector:x=>x==='#motion-status'?status:content,createElement:x=>new E(x)};globalThis.fetch=async url=>new Response(JSON.stringify(url.includes('clearcreek-motion')?value:report));await import(`../clearcreek-motion.js?test=${version}`);return{status,content};}
 const ok=await render(data,'ok');assert.match(ok.status.textContent,/preliminary/);assert.ok(ok.content.children.some(x=>x.textContent.includes('entire consent agenda')));assert.ok(ok.content.children.some(x=>x.textContent.includes('entire consent block')));assert.equal(ok.content.children.find(x=>x.tag==='ul').children.length,3);
 const missing=await render({schema_version:data.schema_version,state:'not_available'},'missing');assert.match(missing.status.textContent,/unavailable/);assert.equal(missing.content.children.length,0);
});
