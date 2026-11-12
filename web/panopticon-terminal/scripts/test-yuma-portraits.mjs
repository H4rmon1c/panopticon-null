import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import {yumaPortraits,yumaPortraitFor,yumaPortraitImage} from '../yuma-portraits-api.js';
const data=JSON.parse(readFileSync(new URL('./fixtures/yuma-motion-valid.json',import.meta.url)));
test('three county portraits bind exact historical attendance roles',()=>{
 assert.equal(data.attendance.filter(yumaPortraitFor).length,3);
 assert.equal(yumaPortraitFor({name:'Scott Weaver',role:'Chairman'}),null);
 assert.equal(yumaPortraitFor({name:'Adam Gates',role:'Commissioner'}),null);
 assert.equal(new Set(yumaPortraits.map(p=>p.sha256)).size,3);
});
test('missing portrait is removed',()=>{
 let error,removed=false;const doc={createElement:()=>({addEventListener:(event,fn)=>{assert.equal(event,'error');error=fn;},remove:()=>{removed=true;}})};
 const img=yumaPortraitImage(doc,data.attendance[0]);assert.ok(img);error();assert.equal(removed,true);
});
test('browser labels historical role and withholds unavailable motion',async()=>{
 class Element{constructor(tag){this.tag=tag;this.children=[];this.textContent='';}append(...children){this.children.push(...children);}addEventListener(event,fn){this[event]=fn;}remove(){this.removed=true;}}
 const research=JSON.parse(readFileSync(new URL('../public/research-signals.json',import.meta.url)));
 async function render(sidecar,report,version){const status=new Element('status'),content=new Element('content');globalThis.document={querySelector:s=>s==='#motion-status'?status:content,createElement:tag=>new Element(tag)};globalThis.fetch=async url=>new Response(JSON.stringify(url.includes('yuma-motion')?sidecar:report));await import(`../yuma-motion.js?test=${version}`);return {status,content};}
 const shown=await render(data,research,'shown');assert.match(shown.status.textContent,/preliminary/);assert.ok(shown.content.children.some(x=>x.textContent.includes('Roles shown are from the 2025 meeting')));const rows=shown.content.children.find(x=>x.tag==='ul').children;assert.equal(rows.length,3);assert.equal(rows.flatMap(x=>x.children.filter(y=>y.tag==='img')).length,3);
 const missing=await render({schema_version:data.schema_version,state:'not_available'},research,'missing');assert.match(missing.status.textContent,/unavailable/);assert.equal(missing.content.children.length,0);
});
