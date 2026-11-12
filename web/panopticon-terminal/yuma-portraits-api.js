// Source profiles identify people; the separate 2025 minutes establish attendance.
export const yumaPortraits=Object.freeze([
 Object.freeze({name:'Adam Gates',role:'Chairman',src:'./assets/portraits/yuma-adam-gates.jpg',sha256:'c38f1a54ba33598461b04221531ab5280b1a71b7486d69a49d8c9810f0a67d26',bytes:91411}),
 Object.freeze({name:'Scott Weaver',role:'Commissioner',src:'./assets/portraits/yuma-scott-weaver.jpg',sha256:'ca38dda41f1b06cf181598f9f1a19885519f477bc4fede820981cc74a0c48c85',bytes:65774}),
 Object.freeze({name:'Mike Leerar',role:'Commissioner (via phone)',src:'./assets/portraits/yuma-mike-leerar.jpg',sha256:'145320cd515685389dc4f01c879aa3b50053d2149fba36a9f85251cceb796514',bytes:78354}),
]);
export function yumaPortraitFor(person){return yumaPortraits.find(p=>person?.name===p.name&&person?.role===p.role)??null;}
export function yumaPortraitImage(document,person){const portrait=yumaPortraitFor(person);if(!portrait)return null;const image=document.createElement('img');image.src=portrait.src;image.alt=`County profile portrait of ${person.name}`;image.width=96;image.height=120;image.loading='lazy';image.decoding='async';image.className='garden-vote__photo';image.addEventListener('error',()=>image.remove());return image;}
