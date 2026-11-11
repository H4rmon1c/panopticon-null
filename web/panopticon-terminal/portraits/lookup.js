// A portrait attaches only to the exact vote-derived voter entity.
export function lookupPortrait(registry, vote, voter) {
  const portrait = registry[vote?.voter_entity_id];
  if (!portrait || voter?.type !== "RECORDED_VOTER" ||
      portrait.name !== vote.name || voter.name !== vote.name || voter.id !== vote.voter_entity_id) return null;
  for (const field of ["jurisdiction_ref", "identity_scope", "official_person_id"]) {
    if (portrait[field] !== undefined || voter[field] !== undefined) {
      if (!portrait[field] || String(portrait[field]) !== String(voter[field])) return null;
    }
  }
  return portrait;
}
