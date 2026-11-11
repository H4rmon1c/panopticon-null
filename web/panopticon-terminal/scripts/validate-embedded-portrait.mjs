// Only a pinned source for this jurisdiction can establish a portrait identity.
export function assertEmbeddedPortraitMatches(record, vote, proofs) {
  const { proof, arapahoe2024, littleton, later, commerce, denver2019, denver2020 } = proofs;
  const candidates = {
    "us-co-county-08005": [proof, arapahoe2024],
    "us-co-place-0845255": [littleton, later],
    "us-co-place-0816495": [commerce],
    "us-co-place-0820000": [denver2019, denver2020],
  }[record.jurisdiction_ref] ?? [];
  const portrait = vote.portrait;
  const selected = candidates.find((item) => item.source_sha256 === portrait.source_document_sha256);
  const source = (selected?.portraits ?? (selected?.portrait ? [selected.portrait] : []))
    .find((item) => item.person_id === vote.person_id && item.name === vote.name);
  const needsOriginal = selected === littleton || selected === denver2020;
  if (!source || portrait.embedded_image_index !== source.image_index || portrait.sha256 !== source.sha256 ||
      portrait.source_url !== selected.source_url || portrait.source_page !== selected.source_page ||
      (needsOriginal && portrait.embedded_original_sha256 !== source.original_sha256)) {
    throw new Error("Embedded portrait identity does not match its source proof");
  }
}
