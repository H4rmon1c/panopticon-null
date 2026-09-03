const DEFAULT_TIMEOUT_MS = 3500;

export class PanopticonClient {
  constructor({ baseUrl = "/api/v1", mockUrl = "./mock/public-snapshot.json" } = {}) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.mockUrl = mockUrl;
    this.mode = "connecting";
    this.snapshot = null;
  }

  async bootstrap() {
    try {
      const [status, activityResponse, featuredResponse] = await Promise.all([
        this.#request("/status"),
        this.#request("/activity?limit=12"),
        this.#request("/entities?limit=6&sort=updated"),
      ]);

      this.mode = "live";
      return {
        mode: this.mode,
        status: normalizeStatus(status),
        activity: normalizeItems(activityResponse),
        featured: normalizeItems(featuredResponse),
      };
    } catch (error) {
      const snapshot = await this.#loadMock();
      this.mode = "demo";
      return {
        mode: this.mode,
        status: normalizeStatus(snapshot.status, snapshot),
        activity: snapshot.activity ?? [],
        featured: snapshot.entities?.slice(0, 6) ?? [],
        fallbackReason: error instanceof Error ? error.message : String(error),
      };
    }
  }

  async search(query, { limit = 12 } = {}) {
    const trimmed = query.trim();
    if (this.mode === "live") {
      try {
        const path = trimmed
          ? `/search?q=${encodeURIComponent(trimmed)}&limit=${limit}`
          : `/entities?limit=${limit}&sort=updated`;
        return normalizeItems(await this.#request(path));
      } catch {
        // Keep the shell usable during API development when a mock snapshot exists.
      }
    }

    const snapshot = await this.#loadMock();
    if (!trimmed) return (snapshot.entities ?? []).slice(0, limit).map(entityResult);
    return searchSnapshot(snapshot, trimmed, limit);
  }

  async getEntity(id) {
    if (this.mode === "live") {
      try {
        return await this.#request(`/entities/${encodeURIComponent(id)}`);
      } catch {
        // Fall through only when a matching demonstration entity exists.
      }
    }

    const snapshot = await this.#loadMock();
    const entity = snapshot.entities?.find((item) => item.id === id);
    if (!entity) throw new Error(`Public entity not found: ${id}`);
    return entity;
  }

  async getEvidence(id) {
    if (this.mode === "live") {
      try {
        return await this.#request(`/evidence/${encodeURIComponent(id)}`);
      } catch {
        // Development fallback below.
      }
    }

    const snapshot = await this.#loadMock();
    const evidence = snapshot.evidence?.find((item) => item.id === id);
    if (!evidence) throw new Error(`Public evidence not found: ${id}`);
    return evidence;
  }

  async getSource(id) {
    if (this.mode === "live") {
      try {
        return await this.#request(`/sources/${encodeURIComponent(id)}`);
      } catch {
        // Development fallback below.
      }
    }

    const snapshot = await this.#loadMock();
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

  async #request(path, options = {}) {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), DEFAULT_TIMEOUT_MS);

    try {
      const response = await fetch(`${this.baseUrl}${path}`, {
        ...options,
        headers: { Accept: "application/json", ...(options.headers ?? {}) },
        credentials: "same-origin",
        signal: controller.signal,
      });

      if (!response.ok) throw new Error(`Public API ${response.status} for ${path}`);
      const contentType = response.headers.get("content-type") ?? "";
      if (!contentType.includes("application/json")) {
        throw new Error(`Public API returned non-JSON content for ${path}`);
      }
      return await response.json();
    } finally {
      clearTimeout(timeout);
    }
  }

  async #loadMock() {
    if (this.snapshot) return this.snapshot;

    const response = await fetch(this.mockUrl, {
      headers: { Accept: "application/json" },
      cache: "no-store",
    });
    if (!response.ok) throw new Error(`Unable to load public snapshot (${response.status})`);

    const snapshot = await response.json();
    validateSnapshot(snapshot);
    this.snapshot = snapshot;
    return snapshot;
  }
}

function normalizeItems(value) {
  if (Array.isArray(value)) return value;
  if (Array.isArray(value?.items)) return value.items;
  return [];
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

function validateSnapshot(snapshot) {
  if (!snapshot || typeof snapshot !== "object") throw new Error("Public snapshot must be an object");
  if (typeof snapshot.schema_version !== "string") throw new Error("Public snapshot is missing schema_version");
  if (!snapshot.status || !Array.isArray(snapshot.entities)) {
    throw new Error("Public snapshot is missing status or entities");
  }
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
