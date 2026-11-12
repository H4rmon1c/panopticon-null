import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {validateOurayMotion,validateOurayResearchBinding} from '../ouray-motion-api.js';
const data=JSON.parse(readFileSync(new URL('./fixtures/ouray-motion-valid.json',import.meta.url)));
const record={id:data.research_record,kind:'meeting_document',historical:false,jurisdiction_ref:data.jurisdiction_ref,source_url:data.source.url,source_sha256:data.source.sha256};
test('unanimous outcome with attendance and no individual votes',()=>{
 validateOurayMotion(data);validateOurayResearchBinding(data,{records:[record]});assert.equal(data.outcome,'Motion passed unanimously');
 assert.equal(data.attendance.length,3);assert.ok(data.attendance.every(x=>x.individual_vote==='Not individually recorded'));assert.equal(data.votes,undefined);
 validateOurayMotion({schema_version:data.schema_version,state:'not_available'});
});
test('invented vote, motion, identity, locator and source changes fail',()=>{
 const edits=[d=>d.attendance[0].individual_vote='Yes',d=>d.attendance[1].name='Other',d=>d.votes=[],d=>d.outcome='Motion failed',d=>d.motion='Body-worn camera contract signed',d=>d.source.motion_page=1,d=>d.source.index_anchor=40,d=>d.source.sha256='0'.repeat(64),d=>d.source.current_index_anchor=-1,d=>d.source.current_index_receipt_sha256='invalid'];
 for(const edit of edits){const copy=structuredClone(data);edit(copy);assert.throws(()=>validateOurayMotion(copy));}
});
test('missing historical or mismatched research records withhold',()=>{
 assert.throws(()=>validateOurayResearchBinding(data,{records:[]}));assert.throws(()=>validateOurayResearchBinding(data,{records:[record,record]}));
 for(const key of Object.keys(record))assert.throws(()=>validateOurayResearchBinding(data,{records:[{...record,[key]:'changed'}]}));
});
