import {validateYumaMotion,validateYumaResearchBinding} from './yuma-motion-api.js?v=1';
import {readBoundedResearchResponse} from './research-api.js?v=garden-motion-1';
const status=document.querySelector('#motion-status'), content=document.querySelector('#motion-content');
const node=(tag,text,cls='')=>{const e=document.createElement(tag);e.textContent=text;if(cls)e.className=cls;return e;};
const link=(url,label)=>{const e=node('a',label,'source');e.href=url;e.target='_blank';e.rel='noopener noreferrer';return e;};
try{
  const response=await fetch('./public/yuma-motion-v1.json',{cache:'no-store'});
  if(!response.ok||!response.body||Number(response.headers.get('content-length')??0)>65536)throw new Error('unavailable');
  const reader=response.body.getReader(),chunks=[];let size=0;
  try{while(true){const {value,done}=await reader.read();if(done)break;size+=value.byteLength;if(size>65536)throw new Error('too large');chunks.push(value);}}finally{await reader.cancel().catch(()=>{});}
  const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}
  const data=validateYumaMotion(JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes)));
  if(data.state!=='available')throw new Error('unavailable');
  validateYumaResearchBinding(data,await readBoundedResearchResponse(await fetch('./public/research-signals.json',{cache:'no-store'})));
  status.textContent='Source-bound preliminary motion; not a released decision record.';
  content.append(node('p',`${data.jurisdiction} · ${data.body} · ${data.meeting_date}`,'meta'));
  content.append(node('h2',data.motion),node('p',data.outcome,'title'),node('p',data.notice,'note'));
  content.append(node('h3','Commissioners present throughout the day'));
  content.append(node('p','Attendance is not an individual roll call on this motion.','meta'));
  const list=node('ul','');for(const person of data.attendance)list.append(node('li',`${person.name} · ${person.role} · ${person.individual_vote}`));content.append(list);
  content.append(link(`${data.source.url}#page=${data.source.motion_page}`,'Open the motion and unanimous outcome, page 2 ↗'));
  content.append(link(`${data.source.url}#page=${data.source.attendance_page}`,'Open the meeting attendance, page 1 ↗'));
  content.append(link(data.source.index_url,'Open the county commissioners’ source index ↗'));
  const proof=document.createElement('details');proof.className='proof';proof.append(node('summary','Source capture details'));
  for(const t of [`Captured ${data.source.captured_at}`,`Attendance: ${data.source.attendance_locator}; motion: ${data.source.motion_locator}.`,
    `PDF SHA-256 ${data.source.sha256}`,`Extracted text SHA-256 ${data.source.text_sha256}`,
    `Original county index anchor ${data.source.index_anchor}; SHA-256 ${data.source.index_sha256}`,
    `Current county index anchor ${data.source.current_index_anchor}; SHA-256 ${data.source.current_index_sha256}`,
    `PDF receipt SHA-256 ${data.source.receipt_sha256}`,`Original index receipt SHA-256 ${data.source.index_receipt_sha256}`,
    `Current index receipt SHA-256 ${data.source.current_index_receipt_sha256}`])proof.append(node('p',t,'digest'));
  content.append(proof);
}catch{status.textContent='This preliminary motion record is currently unavailable.';}
