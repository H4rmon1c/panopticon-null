// Exact city-report image; the meeting minutes separately establish the vote.
export const luceroPortrait = Object.freeze({
  name: 'Orly Lucero', role: 'Councilmember',
  src: './assets/portraits/cortez-orly-lucero-2017.jpg',
  source: 'https://www.cortezco.gov/DocumentCenter/View/775/2017-Budget-in-Brief?bidId=#page=2',
  sha256: '792f36327372957b5cd20b24fa7d138a9a15715f00b085711c9d334b154a4e0c',
  bytes: 9297,
});
export function cortezPortraitFor(vote) {
  return vote?.name === luceroPortrait.name && vote?.role === luceroPortrait.role ? luceroPortrait : null;
}
export function cortezPortraitImage(document, vote) {
  const portrait = cortezPortraitFor(vote);
  if (!portrait) return null;
  const photo = document.createElement('img');
  photo.src = portrait.src; photo.alt = `Historical city-report portrait of ${vote.name}`;
  photo.width = 96; photo.height = 120; photo.loading = 'lazy'; photo.decoding = 'async';
  photo.className = 'garden-vote__photo';
  photo.addEventListener('error', () => photo.remove());
  return photo;
}
