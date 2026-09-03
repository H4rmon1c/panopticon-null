function openPrimaryEvidence() {
  const id = state.selected?.attributes.find((item) => item.evidence_id)?.evidence_id
    ?? state.selected?.relationships.find((item) => item.evidence_id)?.evidence_id;
  if (id) openEvidence(id);
  else toast("NO PUBLIC EVIDENCE ATTACHED");
}

async function openEvidence(id) {
  try {
    const evidence = await client.getEvidence(id);
    const source = await client.getSource(evidence.source_id);
    state.sources.set(source.id, source);
    $("#evidence-id").textContent = evidence.id.toUpperCase();
    $("#evidence-review").textContent = evidence.review_state ?? "UNKNOWN";
    $("#evidence-claim").textContent = evidence.claim ?? "Public claim";
    $("#evidence-quote").textContent = evidence.quote ?? "No exact quote available.";
    $("#evidence-source").textContent = source.title ?? source.id;
    $("#evidence-authority").textContent = evidence.authority ?? source.authority ?? "UNKNOWN";
    $("#evidence-locator").textContent = evidence.locator ?? "UNSPECIFIED";
    $("#evidence-retrieved").textContent = dateTime(evidence.retrieved_at ?? source.retrieved_at);
    $("#evidence-hash").textContent = evidence.sha256 ?? source.sha256 ?? "UNAVAILABLE";
    const link = $("#evidence-link");
    if (source.demo || source.canonical_url?.includes("example.invalid")) {
      link.removeAttribute("href");
      link.textContent = "SYNTHETIC SOURCE // DEMO ONLY";
      link.setAttribute("aria-disabled", "true");
    } else {
      link.href = source.canonical_url;
      link.target = "_blank";
      link.rel = "noopener noreferrer";
      link.textContent = "OPEN PUBLIC SOURCE ↗";
      link.removeAttribute("aria-disabled");
    }
    openEvidenceDrawer();
  } catch (error) {
    toast(`EVIDENCE UNAVAILABLE // ${error.message}`);
  }
}

async function openSource(id) {
  try {
    const source = state.sources.get(id) ?? await client.getSource(id);
    state.sources.set(id, source);
    $("#evidence-id").textContent = source.id.toUpperCase();
    $("#evidence-review").textContent = source.demo ? "DEMO RECORD" : "PUBLIC SOURCE";
    $("#evidence-claim").textContent = source.title;
    $("#evidence-quote").textContent = source.description ?? "Public source record.";
    $("#evidence-source").textContent = source.title;
    $("#evidence-authority").textContent = source.authority ?? "UNKNOWN";
    $("#evidence-locator").textContent = source.source_type ?? "SOURCE RECORD";
    $("#evidence-retrieved").textContent = dateTime(source.retrieved_at);
    $("#evidence-hash").textContent = source.sha256 ?? "UNAVAILABLE";
    const link = $("#evidence-link");
    if (source.demo || source.canonical_url?.includes("example.invalid")) {
      link.removeAttribute("href"); link.textContent = "SYNTHETIC SOURCE // DEMO ONLY";
    } else {
      link.href = source.canonical_url; link.target = "_blank"; link.rel = "noopener noreferrer"; link.textContent = "OPEN PUBLIC SOURCE ↗";
    }
    openEvidenceDrawer();
  } catch (error) {
    toast(`SOURCE UNAVAILABLE // ${error.message}`);
  }
}

function openEvidenceDrawer() {
  const drawer = $("#evidence-drawer");
  drawer.classList.add("is-open");
  drawer.setAttribute("aria-hidden", "false");
}
function closeEvidence() { $("#evidence-drawer").classList.remove("is-open"); $("#evidence-drawer").setAttribute("aria-hidden", "true"); }
async function copyHash() { await copyText($("#evidence-hash").textContent); toast("EVIDENCE HASH COPIED"); }
async function copyPermalink() { await copyText(location.href); toast("PUBLIC PERMALINK COPIED"); }
