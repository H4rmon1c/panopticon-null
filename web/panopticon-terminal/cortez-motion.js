import {validateCortezMotion,validateCortezResearchBinding} from './cortez-motion-api.js?v=1';
import {readBoundedResearchResponse} from './research-api.js?v=garden-motion-1';
import {cortezPortraitFor,cortezPortraitImage} from './cortez-portrait-api.js?v=2';
const status=document.querySelector('#motion-status');const content=document.querySelector('#motion-content');
const node=(tag,text,cls='')=>{const el=document.createElement(tag);el.textContent=text;if(cls)el.className=cls;return el;};
const link=(url,label)=>{const el=node('a',label,'source');el.href=url;el.target='_blank';el.rel='noopener noreferrer';return el;};
try {
  const response=await fetch('./public/cortez-motion-v1.json',{cache:'no-store'});
  if(!response.ok||!response.body||Number(response.headers.get('content-length')??0)>65536)throw new Error('unavailable');
  const reader=response.body.getReader(),chunks=[];let size=0;
  try {while(true){const {value,done}=await reader.read();if(done)break;size+=value.byteLength;if(size>65536)throw new Error('too large');chunks.push(value);}}
  finally {await reader.cancel().catch(()=>{});}
  const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}
  const data=validateCortezMotion(JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes)));
  if(data.state!=='available')throw new Error('unavailable');
  const research=await readBoundedResearchResponse(await fetch('./public/research-signals.json',{cache:'no-store'}));
  validateCortezResearchBinding(data,research);
  status.textContent='Source-bound preliminary motion; not a released decision record.';
  content.append(node('p',`${data.jurisdiction} · ${data.body} · ${data.meeting_date}`,'meta'));
  content.append(node('h2',data.motion));content.append(node('p',data.outcome,'title'));
  content.append(node('p',data.scope,'note'),node('p',data.notice,'note'));
  content.append(node('h3','Recorded votes on final reading'));
  content.append(node('p','When available, portraits come from the linked city report or newspaper articles; photograph dates are unknown. The meeting minutes separately record the vote. Other portraits are unavailable.','meta'));
  const list=node('ul','','garden-votes');
  for(const vote of data.votes){
    const item=node('li','','garden-vote'), portrait=cortezPortraitFor(vote);
    if(portrait)item.append(cortezPortraitImage(document,vote));
    const info=node('div','','garden-vote__details');info.append(node('strong',vote.name),node('p',`${vote.role} · ${vote.position}`,'meta'));
    if(portrait)info.append(node('p',`Photo credit: ${portrait.credit}`,'meta'),link(portrait.source,portrait.label));
    item.append(info);list.append(item);
  }
  content.append(list);
  content.append(link(`${data.source.url}#page=${data.source.vote_page}`,'Open final-reading motion and visual vote table, page 2 ↗'));
  content.append(link(`${data.source.url}#page=${data.source.roster_page}`,'Open meeting roster, page 1 ↗'));
  content.append(link(data.source.index_url,'Open the city’s 2021 meeting index ↗'));
  const proof=document.createElement('details');proof.className='proof';proof.append(node('summary','Source capture details'));
  for(const text of [`Captured ${data.source.captured_at}`,`Roster: ${data.source.roster_locator}; motion: ${data.source.motion_locator}; table: ${data.source.vote_locator}.`,
    `PDF SHA-256 ${data.source.sha256}`,`Extracted text SHA-256 ${data.source.text_sha256}`,
    `City index anchor ${data.source.index_anchor}; SHA-256 ${data.source.index_sha256}`,
    `PDF receipt SHA-256 ${data.source.receipt_sha256}`,`Index receipt SHA-256 ${data.source.index_receipt_sha256}`])proof.append(node('p',text,'digest'));
  content.append(proof);
} catch {status.textContent='This preliminary motion record is currently unavailable.';}
