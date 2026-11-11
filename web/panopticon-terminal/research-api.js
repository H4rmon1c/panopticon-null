// Preliminary source mentions are a separate dataset from verified decisions.
const onlyKeys = (value, keys) => value && typeof value === 'object' && !Array.isArray(value) &&
  Object.keys(value).every((key) => keys.includes(key));
const nonnegative = (value) => Number.isSafeInteger(value) && value >= 0;
const digest = (value) => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const safeText = (value, maximum) => typeof value === 'string' && value.length <= maximum &&
  !/[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/u.test(value);
const locator = (value) => typeof value === 'string' && /^\$\[\d+\]$/.test(value);

function validatePortrait(portrait) {
  if (!onlyKeys(portrait, ['src', 'sha256', 'source_url', 'image_url', 'caption', 'portrait_date']) ||
      !digest(portrait.sha256) ||
      !new RegExp(`^\\./research-portraits/${portrait.sha256}\\.(?:jpg|png|webp)$`).test(portrait.src) ||
      portrait.portrait_date !== 'not_established' || !safeText(portrait.caption, 250)) throw new Error('Invalid research portrait');
  let page, image;
  try { page = new URL(portrait.source_url); image = new URL(portrait.image_url); }
  catch { throw new Error('Invalid portrait source URL'); }
  if (page.protocol !== 'https:' || image.protocol !== 'https:' || page.hostname !== image.hostname ||
      !page.hostname.includes('.') || /^(?:localhost|\[|[0-9.]+$)/i.test(page.hostname) ||
      /\.(?:localhost|local|internal)$/i.test(page.hostname) ||
      page.username || page.password || image.username || image.password || page.port || image.port || page.hash || image.hash) {
    throw new Error('Unsafe portrait source URL');
  }
}

export function validateResearch(report) {
  if (!onlyKeys(report, ['schema_version', 'state', 'generated_at', 'notice', 'coverage', 'records']) ||
      report.schema_version !== 'pnull-research-v1' || report.state !== 'preliminary' ||
      !Array.isArray(report.records) || report.records.length > 50000 ||
      !onlyKeys(report.coverage, ['lookback_period', 'jurisdictions_with_mentions', 'matter_references',
        'document_references', 'agenda_portal_references', 'agenda_portal_screening', 'recorded_actions', 'recorded_vote_entries', 'document_screening',
        'refresh_failures_preserving_prior_results', 'omitted_failed_provenance']) ||
      report.coverage.lookback_period !== null ||
      !onlyKeys(report.coverage.document_screening, ['pending', 'error', 'matched', 'no_match']) ||
      !Object.values(report.coverage.document_screening).every(nonnegative) ||
      (report.coverage.agenda_portal_screening !== undefined &&
        (!onlyKeys(report.coverage.agenda_portal_screening, ['pending', 'error', 'matched', 'no_match']) ||
          !Object.values(report.coverage.agenda_portal_screening).every(nonnegative))) ||
      !nonnegative(report.coverage.omitted_failed_provenance) ||
      (report.coverage.refresh_failures_preserving_prior_results !== undefined &&
        !nonnegative(report.coverage.refresh_failures_preserving_prior_results)) ||
      typeof report.notice !== 'string' || report.notice.length > 500 ||
      /[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/u.test(report.notice) ||
      typeof report.generated_at !== 'string' || !Number.isFinite(Date.parse(report.generated_at))) {
    throw new Error('Invalid preliminary research report');
  }
  const seen = new Set();
  for (const record of report.records) {
    if (!onlyKeys(record, ['id', 'kind', 'confidence', 'jurisdiction_ref', 'jurisdiction', 'file', 'title', 'signals',
      'source_status', 'source_url', 'matter_url', 'source_sha256', 'locator', 'captured_at', 'last_screened_at',
      'people_context', 'historical', 'ocr', 'actions']) ||
        !/^co-[a-z0-9-]{1,150}$/.test(record.id) || seen.has(record.id) ||
        !['matter_reference', 'meeting_document', 'agenda_portal_reference'].includes(record.kind) ||
        record.confidence !== 'unconfirmed_source_reference' ||
        !/^us-co-[a-z0-9-]{1,100}$/.test(record.jurisdiction_ref) ||
        typeof record.jurisdiction !== 'string' || !record.jurisdiction.trim() || record.jurisdiction.length > 300 ||
        typeof record.title !== 'string' || record.title.length > 400 ||
        typeof record.file !== 'string' || record.file.length > 120 ||
        typeof record.source_status !== 'string' || record.source_status.length > 100 ||
        !Array.isArray(record.signals) || !record.signals.length || record.signals.length > 24 ||
        record.signals.some((item) => typeof item !== 'string' || !item.trim() || item.length > 100) ||
        !/^[0-9a-f]{64}$/.test(record.source_sha256) ||
        !/^(?:\$\[\d+\]\.MatterTitle|extracted line [1-9]\d*|HTML text node [1-9]\d*)$/.test(record.locator) ||
        typeof record.captured_at !== 'string' || !Number.isFinite(Date.parse(record.captured_at)) ||
        typeof record.last_screened_at !== 'string' || !Number.isFinite(Date.parse(record.last_screened_at))) {
      throw new Error('Invalid preliminary research record');
    }
    seen.add(record.id);
    for (const value of [record.jurisdiction, record.title, record.file, record.source_status, ...record.signals]) {
      if (/[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/u.test(value)) throw new Error('Unsafe research text');
    }
    const parsedUrls = [];
    for (const value of [record.source_url, ...(record.matter_url ? [record.matter_url] : [])]) {
      if (typeof value !== 'string') throw new Error('Invalid research source URL');
      let url;
      try { url = new URL(value); } catch { throw new Error('Invalid research source URL'); }
      if (url.protocol !== 'https:' || !url.hostname.includes('.') || url.username || url.password || url.hash ||
          url.port || /^(?:localhost|\[|[0-9.]+$)/i.test(url.hostname) || /\.(?:localhost|local|internal)$/i.test(url.hostname) || value.length > 4000) {
        throw new Error('Unsafe research source URL');
      }
      if (record.kind === 'matter_reference' && url.hostname !== 'webapi.legistar.com') {
        throw new Error('Matter reference is not from the official portal API');
      }
      parsedUrls.push(url);
    }
    if (record.kind === 'matter_reference' && !record.matter_url) throw new Error('Matter reference lacks an official record URL');
    if (record.kind === 'matter_reference') {
      const source = /^\/v1\/([a-z0-9-]+)\/matters$/.exec(parsedUrls[0].pathname);
      const matter = /^\/v1\/([a-z0-9-]+)\/Matters\/([1-9]\d*)$/.exec(parsedUrls[1].pathname);
      if (!source || !matter || source[1] !== matter[1] || parsedUrls[1].search ||
          record.id !== `co-legistar-${matter[1]}-${matter[2]}`) throw new Error('Matter identity is inconsistent');
    } else if (record.kind === 'meeting_document' && (!/^co-document-[a-f0-9]{24}$/.test(record.id) || parsedUrls[0].search ||
        !(/\/AgendaCenter\/ViewFile\/(?:Agenda|Minutes)\/_[0-9]{8}-[0-9]{1,12}$/i.test(parsedUrls[0].pathname) ||
          /\.pdf$/i.test(parsedUrls[0].pathname)))) throw new Error('Meeting document URL is inconsistent');
    else if (record.kind === 'agenda_portal_reference') {
      const meeting = /^\/iip\/elpaso\/meeting\/details\/([1-9]\d*)$/.exec(parsedUrls[0].pathname);
      if (parsedUrls[0].hostname !== 'www.agendasuite.org' || parsedUrls[0].search || !meeting ||
          record.jurisdiction_ref !== 'us-co-county-08041' || record.id !== `co-agendasuite-elpaso-${meeting[1]}-${record.source_sha256.slice(0, 12)}` ||
          !/^HTML text node [1-9]\d*$/.test(record.locator)) throw new Error('Agenda portal source is inconsistent');
    }
    if (record.kind !== 'matter_reference' && record.matter_url !== undefined) throw new Error('Unexpected matter URL');
    if (record.kind === 'meeting_document' && record.ocr !== undefined && typeof record.ocr !== 'boolean') throw new Error('Invalid OCR label');
    if (record.historical !== undefined && typeof record.historical !== 'boolean') throw new Error('Invalid historical label');
    if (record.actions !== undefined) {
      if (record.kind !== 'matter_reference' || !Array.isArray(record.actions) || record.actions.length > 10) throw new Error('Invalid matter actions');
      const matter = /^\/v1\/([a-z0-9-]+)\/Matters\/([1-9]\d*)$/.exec(parsedUrls[1].pathname);
      const prefix = `https://webapi.legistar.com/v1/${matter[1]}`;
      for (const motion of record.actions) {
        if (!onlyKeys(motion, ['date', 'body', 'action', 'outcome', 'vote_scope', 'history_url', 'history_sha256',
          'history_locator', 'item_url', 'item_sha256', 'votes_url', 'votes_sha256', 'captured_at', 'votes', 'block_url', 'block_sha256', 'historical']) ||
            !/^(?:\d{4}-\d{2}-\d{2}|not_established)$/.test(motion.date) ||
            !safeText(motion.body, 200) || !safeText(motion.action, 200) || !safeText(motion.outcome, 100) ||
            !['item', 'consent_block', 'consent_designated_item', 'not_established'].includes(motion.vote_scope) ||
            motion.history_url !== `${prefix}/Matters/${matter[2]}/Histories` || !locator(motion.history_locator) ||
            !digest(motion.history_sha256) || !digest(motion.item_sha256) || !digest(motion.votes_sha256) ||
            (motion.historical !== undefined && typeof motion.historical !== 'boolean') ||
            typeof motion.captured_at !== 'string' || !Number.isFinite(Date.parse(motion.captured_at)) ||
            !Array.isArray(motion.votes) || motion.votes.length > 100 ||
            (motion.vote_scope === 'not_established' && motion.votes.length)) throw new Error('Invalid matter action');
        const escaped = matter[1].replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        const item = new RegExp(`^https://webapi\\.legistar\\.com/v1/${escaped}/Events/([1-9]\\d*)/EventItems/([1-9]\\d*)$`).exec(motion.item_url);
        if (!item || motion.votes_url !== `${prefix}/EventItems/${item[2]}/Votes`) throw new Error('Invalid action source identity');
        if (motion.vote_scope === 'consent_block') {
          if (!new RegExp(`^${prefix.replaceAll('.', '\\.')}\\/Events/${item[1]}\\/EventItems/[1-9]\\d*$`).test(motion.block_url) ||
              !digest(motion.block_sha256)) throw new Error('Invalid consent-block evidence');
        } else if (motion.block_url !== undefined || motion.block_sha256 !== undefined) throw new Error('Unexpected consent-block evidence');
        const people = new Set();
        for (const vote of motion.votes) {
          if (!onlyKeys(vote, ['name', 'value', 'locator', 'person_id', 'vote_id', 'portrait']) ||
              !safeText(vote.name, 100) || !vote.name.trim() ||
              !['Aye', 'Yes', 'No', 'Nay', 'Abstain', 'Absent', 'Recused'].includes(vote.value) ||
              !locator(vote.locator) || !Number.isSafeInteger(vote.person_id) || vote.person_id <= 0 ||
              !Number.isSafeInteger(vote.vote_id) || vote.vote_id <= 0 || people.has(vote.person_id)) throw new Error('Invalid recorded vote');
          people.add(vote.person_id);
          if (vote.portrait) validatePortrait(vote.portrait);
        }
      }
    }
    if (record.people_context !== undefined) {
      if (record.kind !== 'meeting_document' || !Array.isArray(record.people_context) || record.people_context.length > 15) throw new Error('Invalid named context');
      for (const person of record.people_context) {
        if (!onlyKeys(person, ['name', 'role_in_source', 'name_locator', 'context_locator', 'individual_vote', 'portrait']) ||
            typeof person.name !== 'string' || !person.name.trim() || person.name.length > 100 ||
            typeof person.role_in_source !== 'string' || person.role_in_source.length > 50 ||
            !/^extracted line [1-9]\d*$/.test(person.name_locator) ||
            !/^extracted line [1-9]\d*$/.test(person.context_locator) || person.individual_vote !== 'not_established' ||
            /[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/u.test(person.name + person.role_in_source)) throw new Error('Invalid named context');
        if (person.portrait) {
          validatePortrait(person.portrait);
        }
      }
    }
  }
  const count = (kind) => report.records.filter((item) => item.kind === kind).length;
  if (report.coverage.matter_references !== count('matter_reference') ||
      report.coverage.document_references !== count('meeting_document') ||
      (report.coverage.agenda_portal_references ?? 0) !== count('agenda_portal_reference') ||
      report.coverage.jurisdictions_with_mentions !== new Set(report.records.map((item) => item.jurisdiction_ref)).size) {
    throw new Error('Research counts do not match records');
  }
  const motions = report.records.flatMap((item) => item.actions ?? []);
  if ((report.coverage.recorded_actions !== undefined && report.coverage.recorded_actions !== motions.length) ||
      (report.coverage.recorded_vote_entries !== undefined && report.coverage.recorded_vote_entries !== motions.reduce((sum, motion) => sum + motion.votes.length, 0))) {
    throw new Error('Research action counts do not match records');
  }
  return report;
}

export async function readBoundedResearchResponse(response, maximum = 8 * 1024 * 1024) {
  if (!response.ok || !response.body || !Number.isSafeInteger(maximum) || maximum <= 0) {
    throw new Error('Research response unavailable');
  }
  const declared = Number(response.headers.get('content-length') ?? 0);
  if (!Number.isFinite(declared) || declared < 0 || declared > maximum) throw new Error('Research response exceeds its limit');
  const reader = response.body.getReader();
  const chunks = [];
  let size = 0;
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > maximum) throw new Error('Research response exceeds its limit');
      chunks.push(value);
    }
  } finally { await reader.cancel().catch(() => {}); }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  return validateResearch(JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)));
}
