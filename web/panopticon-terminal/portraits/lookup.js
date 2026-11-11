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

// Preliminary records can reuse an already sourced portrait only when the
// vote's exact portal person ID, jurisdiction, client, and name all agree.
export function lookupResearchPortrait(registry, client, jurisdiction, vote) {
  if (!vote || !Number.isSafeInteger(vote.person_id) || vote.person_id <= 0) return null;
  const found = Object.values(registry).filter((portrait) => portrait.identity_scope === `legistar:${client}` &&
    portrait.jurisdiction_ref === jurisdiction && portrait.official_person_id === String(vote.person_id) &&
    portrait.name === vote.name);
  return found.length === 1 ? found[0] : null;
}

// Bind the Denver gallery's exact labeled image to the voter; a filename or
// a loosely similar name is never enough. Middle initials may be omitted.
export function matchesDenverGallery(portrait, voter, html) {
  try {
    const expected = "https://www.denvergov.org/Government/Agencies-Departments-Offices/Agencies-Departments-Offices-Directory/Denver-City-Council/Press-Room/2023councilofficialphotos";
    const name = (value) => value.trim().replace(/\s+[A-Za-z]\.\s+/, " ").toLowerCase();
    const rows = [...html.matchAll(/<a\b[^>]*href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/gi)]
      .map((match) => ({ href: match[1], body: match[2],
        alt: /<img\b[^>]*\balt="([^"]+)"/i.exec(match[2])?.[1],
        src: /<img\b[^>]*\bsrc="([^"]+)"/i.exec(match[2])?.[1] }))
      .filter((row) => row.alt && name(row.alt.split(",")[0]) === name(portrait.name));
    const row = rows[0];
    const display = row && new URL(row.href, portrait.source_url);
    const thumbnail = row?.src && new URL(row.src, portrait.source_url);
    const original = new URL(portrait.image_url);
    const versionless = (path) => path.replace(/\/v\/\d+\//, "/v/VERSION/");
    const locator = row && `a[href="${row.href}"] > figure img[alt="${row.alt}"]`;
    return portrait.source_url === expected && voter.identity_scope === "legistar:denver" &&
      /<h1\b[^>]*>2023 Official Photos - City Council Members \(16:9\)<\/h1>/i.test(html) &&
      rows.length === 1 && /, (District \d+|At-large)$/.test(row?.alt ?? "") &&
      row.alt === portrait.source_image_alt && locator === portrait.source_page_locator &&
      display?.href === portrait.display_image_url && thumbnail?.pathname === display?.pathname &&
      original.hostname === display?.hostname && !original.search && !original.hash &&
      versionless(original.pathname) === versionless(display?.pathname ?? "");
  } catch {
    return false;
  }
}
