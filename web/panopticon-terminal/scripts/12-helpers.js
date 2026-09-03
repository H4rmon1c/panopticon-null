function uniqueLinks(entities) { const allowed=new Set(entities.map((e)=>e.id)),seen=new Set(),out=[];entities.forEach((entity)=>entity.relationships.forEach((link)=>{if(!allowed.has(link.target_entity_id))return;const key=[entity.id,link.target_entity_id].sort().join("::")+`::${link.evidence_id??link.type}`;if(seen.has(key))return;seen.add(key);out.push({...link,source_entity_id:entity.id});}));return out; }
function averageConfidence(entity){return entity.relationships.length?entity.relationships.reduce((s,item)=>s+Number(item.confidence??0),0)/entity.relationships.length:entity.attributes.length?1:0;}
function validGeo(geo){return geo&&Number.isFinite(Number(geo.lat))&&Number.isFinite(Number(geo.lon));}
function fallbackGeo(index,total){const angle=index/Math.max(total,1)*Math.PI*2;return{lat:38+Math.sin(angle)*12,lon:-105+Math.cos(angle)*22,label:"Approximate public graph placement"};}
function basis(lon,lat){const a=rad(lon),b=rad(lat);return{forward:[Math.cos(b)*Math.cos(a),Math.cos(b)*Math.sin(a),Math.sin(b)],right:[-Math.sin(a),Math.cos(a),0],up:[-Math.sin(b)*Math.cos(a),-Math.sin(b)*Math.sin(a),Math.cos(b)]};}
function vector(lat,lon){const a=rad(lat),b=rad(lon);return[Math.cos(a)*Math.cos(b),Math.cos(a)*Math.sin(b),Math.sin(a)];}
function slerp(a,b,t,omega){if(omega<1e-5)return a;const s=Math.sin(omega),x=Math.sin((1-t)*omega)/s,y=Math.sin(t*omega)/s;return[a[0]*x+b[0]*y,a[1]*x+b[1]*y,a[2]*x+b[2]*y];}
function marker(c,shape,x,y,s){c.beginPath();if(shape==="circle")c.arc(x,y,s*.68,0,Math.PI*2);else if(shape==="diamond"){c.moveTo(x,y-s);c.lineTo(x+s,y);c.lineTo(x,y+s);c.lineTo(x-s,y);c.closePath();}else if(shape==="triangle"){c.moveTo(x,y-s);c.lineTo(x+s*.9,y+s*.8);c.lineTo(x-s*.9,y+s*.8);c.closePath();}else if(shape==="document")c.rect(x-s*.7,y-s,s*1.4,s*2);else c.rect(x-s*.72,y-s*.72,s*1.44,s*1.44);}
function linkColor(type,sensor){if(sensor==="night")return"#8fffaa";if(sensor==="change")return/POWER|SUPPL|EXECUT/i.test(type)?"#ffdc67":"#ff8650";if(/POWER|UTILITY|EXECUT|GOVERN/i.test(type))return"#ffbd37";if(/NETWORK|CONNECT/i.test(type))return"#78a7ff";if(/BUILD|CONTRACT|SUPPL/i.test(type))return"#8fffb1";return"#64e7ff";}
function seeded(count,seed){const out=[];for(let i=0;i<count;i++){let t=seed+=0x6D2B79F5;t=Math.imul(t^t>>>15,t|1);t^=t+Math.imul(t^t>>>7,t|61);out.push(((t^t>>>14)>>>0)/4294967296);}return out;}
function circularMean(values){const list=values.map(rad),x=list.reduce((s,v)=>s+Math.cos(v),0),y=list.reduce((s,v)=>s+Math.sin(v),0);return Math.atan2(y,x)*180/Math.PI;}
function dot(a,b){return a[0]*b[0]+a[1]*b[1]+a[2]*b[2];}
function rad(value){return Number(value)*Math.PI/180;}
function clamp(value,min,max){return Math.min(max,Math.max(min,value));}
function normalizeLon(value){return((value+180)%360+360)%360-180;}
function lerpAngle(a,b,t){return a+normalizeLon(b-a)*t;}
function alpha(hex,a){const h=hex.replace("#","");const v=parseInt(h.length===3?h.split("").map((x)=>x+x).join(""):h,16);return`rgba(${v>>16&255},${v>>8&255},${v&255},${clamp(a,0,1)})`;}
function coord(value,pos,neg){const n=Number(value);return`${Math.abs(n).toFixed(2)}${n>=0?pos:neg}`;}
function relative(value){const delta=Math.max(0,Date.now()-new Date(value).getTime()),minutes=Math.floor(delta/60000);if(minutes<1)return"NOW";if(minutes<60)return`${minutes}M AGO`;const hours=Math.floor(minutes/60);if(hours<48)return`${hours}H AGO`;return`${Math.floor(hours/24)}D AGO`;}
function timeOnly(value){const date=new Date(value);return Number.isFinite(date.getTime())?date.toISOString().slice(11,19):"--:--:--";}
function dateTime(value){const date=new Date(value);return Number.isFinite(date.getTime())?date.toISOString().replace("T"," ").slice(0,16)+" UTC":"UNKNOWN";}
function shortDate(value){return new Intl.DateTimeFormat("en-US",{month:"short",day:"2-digit",year:"numeric",timeZone:"UTC"}).format(new Date(value)).toUpperCase();}
function escapeHtml(value){return String(value??"").replaceAll("&","&amp;").replaceAll("<","&lt;").replaceAll(">","&gt;").replaceAll('"',"&quot;").replaceAll("'","&#039;");}
async function copyText(value){if(navigator.clipboard?.writeText)return navigator.clipboard.writeText(value);const area=document.createElement("textarea");area.value=value;document.body.append(area);area.select();document.execCommand("copy");area.remove();}
function toast(message){const node=$("#toast");node.textContent=message;node.classList.add("is-visible");clearTimeout(toastTimer);toastTimer=setTimeout(()=>node.classList.remove("is-visible"),2200);}
