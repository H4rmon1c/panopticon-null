// These public profile attributions are bound to the independently recorded vote.
const entries = {
  "Fil Archuleta": {
    "vote_role": "Mayor",
    "vote_locator": "extracted line 210",
    "profile_role": "Mayor",
    "profile_url": "https://www.townofgardencity.com/fil-archuleta",
    "profile_sha256": "67f10c6fe68eb3c5f7360063ae7c6da2f7e7aaa33b2db78c355d02b7699df2a3",
    "image_sha256": "4f44e3f3a81340b38ee28fdec5c494d4da4151f25749c5b100878f2c85d70941",
    "image_bytes": 152121,
    "width": 1491,
    "height": 2000,
    "src": "./garden-portraits/4f44e3f3a81340b38ee28fdec5c494d4da4151f25749c5b100878f2c85d70941.jpg"
  },
  "Gary Sorensen": {
    "vote_role": "Trustee",
    "vote_locator": "extracted line 211",
    "profile_role": "Mayor Pro Tem",
    "profile_url": "https://www.townofgardencity.com/gary-sorensen",
    "profile_sha256": "d88e4d51b30f8ad7ffeaaf213d6645746abdae2a4980d7f8d9a57c0f3e47bf62",
    "image_sha256": "4cf994abad184061fbda3aa82a4f9a747e08bb3ee8196d4ee0ab18a6fd71239c",
    "image_bytes": 241296,
    "width": 1491,
    "height": 2000,
    "src": "./garden-portraits/4cf994abad184061fbda3aa82a4f9a747e08bb3ee8196d4ee0ab18a6fd71239c.jpg"
  },
  "Alex Lopez": {
    "vote_role": "Trustee",
    "vote_locator": "extracted line 210",
    "profile_role": "Trustee",
    "profile_url": "https://www.townofgardencity.com/alex-lopez",
    "profile_sha256": "f27d80276628b7010269a96ee391a05249f0fbeea38af13c81d957a57b9c189a",
    "image_sha256": "ba132238ef8f1c56aa49c3d2154a3b83bda2421ade51a211682606d4a00fac93",
    "image_bytes": 159885,
    "width": 1506,
    "height": 2000,
    "src": "./garden-portraits/ba132238ef8f1c56aa49c3d2154a3b83bda2421ade51a211682606d4a00fac93.jpg"
  },
  "Leigh Sorensen": {
    "vote_role": "Trustee",
    "vote_locator": "extracted line 210",
    "profile_role": "Trustee",
    "profile_url": "https://www.townofgardencity.com/leigh-sorensen",
    "profile_sha256": "df03eb70c30daae045dbc299ba5e1e848fe06d679051ab0d3bd647805edd51c0",
    "image_sha256": "0b0bc507b63defc73e03795b83ac2bdc206a5ca28b65d561a25a574be93e6ea2",
    "image_bytes": 225348,
    "width": 1502,
    "height": 2000,
    "src": "./garden-portraits/0b0bc507b63defc73e03795b83ac2bdc206a5ca28b65d561a25a574be93e6ea2.jpg"
  },
  "Katherine Rodriguez": {
    "vote_role": "Trustee",
    "vote_locator": "extracted line 211",
    "profile_role": "Trustee",
    "profile_url": "https://www.townofgardencity.com/katherine-rodriguez",
    "profile_sha256": "77072539bee3949f0e4e8210ff2e75a95d3e10db000a7c8b3bba736c17e8c204",
    "image_sha256": "5c0af4e74784a4b4566d8d7a61303dfe1d684e0326c96723357c076ebd9a7564",
    "image_bytes": 161028,
    "width": 1502,
    "height": 2000,
    "src": "./garden-portraits/5c0af4e74784a4b4566d8d7a61303dfe1d684e0326c96723357c076ebd9a7564.jpg"
  },
  "Jasmine Marquez": {
    "vote_role": "Trustee",
    "vote_locator": "extracted line 211",
    "profile_role": "Trustee",
    "profile_url": "https://www.townofgardencity.com/jasmine-marquez",
    "profile_sha256": "068d68a2c962ba3e00690e7e7fe61f6b3595fcc3be2b27760b321504d8bae8af",
    "image_sha256": "ed5f53d3c855369be1f9f43736c2af146743b0fcbf5f752e0baa885d9fd32d69",
    "image_bytes": 158329,
    "width": 1514,
    "height": 2000,
    "src": "./garden-portraits/ed5f53d3c855369be1f9f43736c2af146743b0fcbf5f752e0baa885d9fd32d69.jpg"
  }
};
export const gardenPortraits = Object.freeze(Object.fromEntries(
  Object.entries(entries).map(([name, item]) => [name, Object.freeze(item)])
));

export function gardenPortraitFor(vote) {
  if (!vote || typeof vote.name !== 'string' || !Object.hasOwn(gardenPortraits, vote.name)) return null;
  const portrait = gardenPortraits[vote.name];
  return vote.role === portrait.vote_role && vote.position === 'Yes' && vote.locator === portrait.vote_locator
    ? portrait : null;
}

export function validateGardenPortraitProof(proof) {
  if (!proof || proof.jurisdiction_ref !== 'us-co-place-0829185' || !Array.isArray(proof.portraits) ||
      proof.portraits.length !== Object.keys(gardenPortraits).length) throw new Error('Invalid Garden City portrait proof');
  const seen = new Set();
  for (const row of proof.portraits) {
    if (!row || typeof row.name !== 'string' || !Object.hasOwn(gardenPortraits, row.name) || seen.has(row.name))
      throw new Error('Invalid Garden City portrait identity');
    seen.add(row.name);
    const expected = gardenPortraits[row.name];
    if (row.source_profile !== expected.profile_url || row.profile_role_at_capture !== expected.profile_role ||
        row.profile_sha256 !== expected.profile_sha256 || row.image_sha256 !== expected.image_sha256 ||
        row.image_bytes !== expected.image_bytes || row.width !== expected.width || row.height !== expected.height ||
        row.local_filename !== expected.image_sha256 + '.jpg' || row.format !== 'JPEG')
      throw new Error('Garden City portrait source differs');
  }
  return proof;
}
