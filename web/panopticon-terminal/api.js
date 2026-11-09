const SNAPSHOT_SCHEMA = "pnull-public-snapshot-v1";

export class PanopticonClient {
  constructor({
    publicUrl = "./public/public-snapshot.json",
    demoUrl = "./mock/public-snapshot.json",
    previewUrl = "./preview-snapshot.json",
    demo = false,
    preview = false,
  } = {}) {
    if (demo && preview) throw new Error("Choose either demonstration or preview data");
    this.publicUrl = publicUrl;
    this.demoUrl = demoUrl;
    this.previewUrl = previewUrl;
    this.demo = demo;
    this.preview = preview;
    this.mode = "connecting";
    this.snapshot = null;
  }

  async bootstrap() {
    const snapshot = await this.#loadSnapshot();
    this.mode = this.demo ? "demo" : this.preview ? "preview" : "public";
    return {
      mode: this.mode,
      status: normalizeStatus(snapshot.status, snapshot),
      activity: snapshot.activity ?? [],
      featured: snapshot.entities?.slice(0, 6) ?? [],
    };
  }

  async search(query, { limit = 12 } = {}) {
    const snapshot = await this.#loadSnapshot();
    const trimmed = query.trim();
    if (!trimmed) return (snapshot.entities ?? []).slice(0, limit).map(entityResult);
    return searchSnapshot(snapshot, trimmed, limit);
  }

  async getEntity(id) {
    const snapshot = await this.#loadSnapshot();
    const entity = snapshot.entities?.find((item) => item.id === id);
    if (!entity) throw new Error(`Public entity not found: ${id}`);
    return entity;
  }

  async getEvidence(id) {
    const snapshot = await this.#loadSnapshot();
    const evidence = snapshot.evidence?.find((item) => item.id === id);
    if (!evidence) throw new Error(`Public evidence not found: ${id}`);
    return evidence;
  }

  async getSource(id) {
    const snapshot = await this.#loadSnapshot();
    const source = snapshot.sources?.find((item) => item.id === id);
    if (!source) throw new Error(`Public source not found: ${id}`);
    return source;
  }

  async getEntitiesByIds(ids) {
    const unique = [...new Set(ids)].filter(Boolean);
    const results = await Promise.allSettled(unique.map((id) => this.getEntity(id)));
    return results
      .filter((result) => result.status === "fulfilled")
      .map((result) => result.value);
  }

  async #loadSnapshot() {
    if (this.snapshot) return this.snapshot;
    const response = await fetch(this.demo ? this.demoUrl : this.preview ? this.previewUrl : this.publicUrl, {
      headers: { Accept: "application/json" },
      cache: "no-store",
    });
    if (!response.ok) throw new Error(`Unable to load ${this.demo ? "demo" : this.preview ? "private preview" : "released public"} snapshot (${response.status})`);

    const snapshot = await response.json();
    validateSnapshot(snapshot, { demo: this.demo, preview: this.preview });
    this.snapshot = snapshot;
    return snapshot;
  }
}

function normalizeStatus(status = {}, snapshot = {}) {
  const counts = status.counts ?? {
    entities: status.entities,
    records: status.records,
    sources: status.sources,
    relationships: status.relationships,
  };

  return {
    state: status.state ?? status.status ?? "ok",
    schema_version: status.schema_version ?? snapshot.schema_version ?? "unknown",
    dataset_version: status.dataset_version ?? "unknown",
    last_ingest: status.last_ingest ?? null,
    last_publish: status.last_publish ?? null,
    manifest_digest: status.manifest_digest ?? "unavailable",
    counts: {
      entities: Number(counts.entities ?? 0),
      records: Number(counts.records ?? 0),
      sources: Number(counts.sources ?? 0),
      relationships: Number(counts.relationships ?? counts.relations ?? 0),
    },
  };
}

export function validateSnapshot(snapshot, { demo = false, preview = false } = {}) {
  if (!snapshot || typeof snapshot !== "object") throw new Error("Public snapshot must be an object");
  if (snapshot.schema_version !== SNAPSHOT_SCHEMA) throw new Error("Unsupported public snapshot schema");
  if (snapshot.demo !== demo) throw new Error("Public and demonstration datasets must stay separate");
  if (Boolean(snapshot.preview) !== preview) throw new Error("Preview and published datasets must stay separate");
  if (!snapshot.status || !Array.isArray(snapshot.entities) || !Array.isArray(snapshot.evidence) || !Array.isArray(snapshot.sources)) {
    throw new Error("Public snapshot is missing status, entities, evidence, or sources");
  }
  if (snapshot.activity && !Array.isArray(snapshot.activity)) throw new Error("Public activity must be a list");
  const hasRecords = snapshot.entities.length + snapshot.evidence.length + snapshot.sources.length + (snapshot.activity?.length ?? 0) > 0;
  if (preview && (snapshot.status.state !== "pending_release" || snapshot.release_review?.state !== "PENDING_RELEASE")) {
    throw new Error("Private preview lacks a pending case release");
  }
  if (!demo && !preview && hasRecords && (snapshot.status.state !== "released" || snapshot.release_review?.state !== "APPROVED" || !snapshot.release_review.reviewed_at)) {
    throw new Error("Public snapshot lacks an approved case release");
  }
  if (!demo && !preview && snapshot.status.state === "empty" && hasRecords) {
    throw new Error("Empty public snapshot contains records");
  }
  if (!demo && (preview || hasRecords) && !/^sha256:[a-f0-9]{64}$/.test(snapshot.release_review?.bound_digest ?? "")) {
    throw new Error("Case release lacks a bound digest");
  }

  const entities = new Set(snapshot.entities.map((entity) => entity.id));
  const sources = new Map(snapshot.sources.map((source) => [source.id, source]));
  const evidence = new Map(snapshot.evidence.map((record) => [record.id, record]));
  if (entities.size !== snapshot.entities.length || sources.size !== snapshot.sources.length || evidence.size !== snapshot.evidence.length) {
    throw new Error("Public snapshot contains duplicate IDs");
  }

  for (const source of snapshot.sources) {
    if (!source.id || !/^[a-f0-9]{64}$/.test(source.sha256 ?? "")) throw new Error(`Source ${source.id ?? "?"} lacks a preserved digest`);
    if (Boolean(source.demo) !== demo) throw new Error(`Source ${source.id} crosses the demonstration boundary`);
    if (!safePublicUrl(source.canonical_url)) throw new Error(`Source ${source.id} lacks a public HTTPS URL`);
  }
  for (const record of snapshot.evidence) {
    if (Boolean(record.demo) !== demo) throw new Error(`Evidence ${record.id ?? "?"} crosses the demonstration boundary`);
    const expectedState = demo ? "DEMO" : preview ? "PENDING_RELEASE" : "SOURCE_VERIFIED";
    if (!record.id || !sources.has(record.source_id) || record.review_state !== expectedState || !record.locator) {
      throw new Error(`Evidence ${record.id ?? "?"} has an invalid source-verification state`);
    }
    if (!/^sha256:[a-f0-9]{64}$/.test(record.verification_digest ?? "") || record.sha256 !== sources.get(record.source_id).sha256) {
      throw new Error(`Evidence ${record.id} lacks a matching verification digest`);
    }
  }
  for (const entity of snapshot.entities) {
    if (!entity.id || !entity.type || !entity.name) throw new Error("Public entity lacks an ID, type, or name");
    if (!demo && (entity.tags ?? []).includes("DEMO DATA")) throw new Error(`Entity ${entity.id} crosses the demonstration boundary`);
    for (const id of entity.source_ids ?? []) if (!sources.has(id)) throw new Error(`Entity ${entity.id} has an unknown source`);
    for (const relation of entity.relationships ?? []) {
      if (!entities.has(relation.target_entity_id) || !evidence.has(relation.evidence_id)) {
        throw new Error(`Entity ${entity.id} has an unsupported relationship`);
      }
      if (!demo && relation.review_state !== (preview ? "PENDING_RELEASE" : "SOURCE_VERIFIED")) throw new Error(`Entity ${entity.id} has an invalid relationship verification state`);
    }
    for (const item of [...(entity.attributes ?? []), ...(entity.timeline ?? [])]) {
      if (item.evidence_id && !evidence.has(item.evidence_id)) throw new Error(`Entity ${entity.id} has unknown evidence`);
      if (!demo && item.review_state !== (preview ? "PENDING_RELEASE" : "SOURCE_VERIFIED")) throw new Error(`Entity ${entity.id} has an invalid fact verification state`);
    }
    for (const vote of entity.votes ?? []) {
      if (!evidence.has(vote.evidence_id)) throw new Error(`Entity ${entity.id} has a vote without evidence`);
      if (!demo && vote.review_state !== (preview ? "PENDING_RELEASE" : "SOURCE_VERIFIED")) throw new Error(`Entity ${entity.id} has an invalid vote verification state`);
    }
    for (const context of entity.technology_context ?? []) {
      if (!evidence.has(context.evidence_id)) throw new Error(`Entity ${entity.id} has technology context without evidence`);
      if (!demo && context.review_state !== (preview ? "PENDING_RELEASE" : "SOURCE_VERIFIED")) throw new Error(`Entity ${entity.id} has an invalid technology-context verification state`);
    }
  }
  return snapshot;
}

function safePublicUrl(value) {
  try { return new URL(value).protocol === "https:"; } catch { return false; }
}

function entityResult(entity) {
  return {
    kind: "entity",
    id: entity.id,
    type: entity.type,
    name: entity.name,
    subtitle: entity.subtitle ?? entity.description ?? "",
    updated_at: entity.updated_at,
    source_count: entity.source_count ?? entity.source_ids?.length ?? 0,
  };
}

function sourceResult(source) {
  return {
    kind: "source",
    id: source.id,
    type: "SOURCE",
    name: source.title,
    subtitle: `${source.authority ?? "UNKNOWN AUTHORITY"} · ${source.source_type ?? "RECORD"}`,
    updated_at: source.retrieved_at,
    source_count: 1,
  };
}

function searchSnapshot(snapshot, query, limit) {
  const parsed = parseQuery(query);
  const terms = parsed.freeText.toLocaleLowerCase().split(/\s+/).filter(Boolean);

  const entities = (snapshot.entities ?? [])
    .filter((entity) => {
      if (parsed.type && entity.type.toLocaleLowerCase() !== parsed.type) return false;
      const haystack = [
        entity.name,
        entity.type,
        entity.subtitle,
        entity.description,
        ...(entity.aliases ?? []),
        ...(entity.tags ?? []),
        ...(entity.attributes ?? []).flatMap((attribute) => [attribute.label, attribute.value]),
      ]
        .filter(Boolean)
        .join(" ")
        .toLocaleLowerCase();
      return terms.length === 0 || terms.every((term) => haystack.includes(term));
    })
    .map((entity) => ({ score: scoreMatch(entity.name, terms) + 10, result: entityResult(entity) }));

  const sources = (snapshot.sources ?? [])
    .filter((source) => {
      if (parsed.type && parsed.type !== "source") return false;
      if (parsed.source && !(source.authority ?? "").toLocaleLowerCase().includes(parsed.source)) return false;
      const haystack = [source.title, source.authority, source.source_type, source.description]
        .filter(Boolean)
        .join(" ")
        .toLocaleLowerCase();
      return terms.length === 0 || terms.every((term) => haystack.includes(term));
    })
    .map((source) => ({ score: scoreMatch(source.title, terms), result: sourceResult(source) }));

  return [...entities, ...sources]
    .sort((a, b) => b.score - a.score || a.result.name.localeCompare(b.result.name))
    .slice(0, limit)
    .map((entry) => entry.result);
}

function parseQuery(query) {
  const filters = {};
  const free = [];

  for (const token of query.match(/(?:[^\s"]+|"[^"]*")+/g) ?? []) {
    const match = token.match(/^([a-z_]+):(.*)$/i);
    if (!match) {
      free.push(token.replace(/^"|"$/g, ""));
      continue;
    }

    const [, key, rawValue] = match;
    const value = rawValue.replace(/^"|"$/g, "").toLocaleLowerCase();
    if (key === "type" || key === "source") filters[key] = value;
    else free.push(token);
  }

  return { type: filters.type, source: filters.source, freeText: free.join(" ") };
}

function scoreMatch(name = "", terms = []) {
  const normalized = name.toLocaleLowerCase();
  return terms.reduce((score, term) => {
    if (normalized === term) return score + 100;
    if (normalized.startsWith(term)) return score + 30;
    if (normalized.includes(term)) return score + 10;
    return score;
  }, 0);
}
