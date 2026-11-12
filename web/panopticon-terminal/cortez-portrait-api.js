// The historical image sources identify the pictured person; the 2021 vote is separate.
export const luceroPortrait = Object.freeze({name:'Orly Lucero',role:'Councilmember',
  src:'./assets/portraits/cortez-orly-lucero-2017.jpg',
  source:'https://www.cortezco.gov/DocumentCenter/View/775/2017-Budget-in-Brief?bidId=#page=2',
  label:'City of Cortez 2017 report and portrait ↗', credit:'City of Cortez 2017 report',
  sha256:'792f36327372957b5cd20b24fa7d138a9a15715f00b085711c9d334b154a4e0c',bytes:9297});
export const cortezPortraits = Object.freeze([
  luceroPortrait,
  Object.freeze({name:'Amy Huckins',role:'Councilmember',src:'./assets/portraits/cortez-amy-huckins-2021.jpg',
    source:'https://www.the-journal.com/amy-huckins-steps-down-from-city-council-position/',
    label:'The Journal, April 2021 ↗',credit:'The Journal file',
    sha256:'ed7d705204895df827cdc9c112da49195bc3cf1b13661209ec730f71aa7671ad',bytes:157541}),
  Object.freeze({name:'Rachel Medina',role:'Mayor Pro Tem',src:'./assets/portraits/cortez-rachel-medina-2022.jpg',
    source:'https://www.the-journal.com/articles/meet-rachel-medina-cortezs-new-mayor/',
    label:'The Journal, May 2022 ↗',credit:'Courtesy photo, via The Journal',
    sha256:'1ac177816727bf52291eacff0dedf04b141f675e651720da12bcd47a6b207391',bytes:48057}),
  Object.freeze({name:'Robert Dobry',role:'Councilmember',src:'./assets/portraits/cortez-robert-dobry-2020.jpg',
    source:'https://www.the-journal.com/articles/news/cortez-city-council-appoints-new-member/',
    label:'The Journal, October 2020 ↗',credit:'The Journal file',
    sha256:'324f0c80bb0e9828175f5706af1fb851f6c4fa8f1d74c4b9f8d1f02447bd280f',bytes:45717}),
]);
export function cortezPortraitFor(vote) {
  return cortezPortraits.find(p => vote?.name===p.name && vote?.role===p.role) ?? null;
}
export function cortezPortraitImage(document,vote) {
  const portrait=cortezPortraitFor(vote);if(!portrait)return null;
  const photo=document.createElement('img');photo.src=portrait.src;photo.alt=`Historical portrait of ${vote.name}`;
  photo.width=96;photo.height=120;photo.loading='lazy';photo.decoding='async';photo.className='garden-vote__photo';
  photo.addEventListener('error',()=>photo.remove());return photo;
}
