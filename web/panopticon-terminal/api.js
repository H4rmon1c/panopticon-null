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
    if (!response.ok) throw new Error(`Unable to load ${this.demo ? "demo" : this.preview ? "private preview" : "reviewed public"} snapshot (${response.status})`);

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
  if (!preview && /pending|draft|rejected/i.test(String(snapshot.status?.state ?? ""))) {
    throw new Error("Pending-review data cannot be loaded as a published snapshot");
  }
  if (!snapshot.status || !Array.isArray(snapshot.entities) || !Array.isArray(snapshot.evidence) || !Array.isArray(snapshot.sources)) {
    throw new Error("Public snapshot is missing status, entities, evidence, or sources");
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
    if (!record.id || !sources.has(record.source_id) || (!preview && record.review_state !== "APPROVED") || !record.locator) {
      throw new Error(`Evidence ${record.id ?? "?"} is not publication ready`);
    }
    if (!/^sha256:[a-f0-9]{64}$/.test(record.review_bound_digest ?? "") || record.sha256 !== sources.get(record.source_id).sha256) {
      throw new Error(`Evidence ${record.id} lacks a matching review-bound digest`);
    }
  }
  for (const entity of snapshot.entities) {
    if (!entity.id || !entity.type || !entity.name) throw new Error("Public entity lacks an ID, type, or name");
    if (!demo && (entity.tags ?? []).includes("DEMO DATA")) throw new Error(`Entity ${entity.id} crosses the demonstration boundary`);
    if (!preview && /pending|draft|rejected/i.test(String(entity.review_state ?? entity.publication_state ?? ""))) {
      throw new Error(`Entity ${entity.id} is not publication ready`);
    }
    for (const id of entity.source_ids ?? []) if (!sources.has(id)) throw new Error(`Entity ${entity.id} has an unknown source`);
    for (const relation of entity.relationships ?? []) {
      if (!entities.has(relation.target_entity_id) || !evidence.has(relation.evidence_id)) {
        throw new Error(`Entity ${entity.id} has an unsupported relationship`);
      }
    }
    for (const item of [...(entity.attributes ?? []), ...(entity.timeline ?? [])]) {
      if (item.evidence_id && !evidence.has(item.evidence_id)) throw new Error(`Entity ${entity.id} has unknown evidence`);
    }
    for (const vote of entity.votes ?? []) {
      if (!evidence.has(vote.evidence_id)) throw new Error(`Entity ${entity.id} has a vote without evidence`);
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
