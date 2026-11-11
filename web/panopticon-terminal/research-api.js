// Preliminary source mentions are a separate dataset from verified decisions.
const onlyKeys = (value, keys) => value && typeof value === 'object' && !Array.isArray(value) &&
  Object.keys(value).every((key) => keys.includes(key));
const nonnegative = (value) => Number.isSafeInteger(value) && value >= 0;
const digest = (value) => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const safeText = (value, maximum) => typeof value === 'string' && value.length <= maximum &&
  !/[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/u.test(value);
const locator = (value) => typeof value === 'string' && /^\$\[\d+\]$/.test(value);
const COUNTY_DOCUMENT_SCOPE_NOTICE = 'The county website links this document. The link alone does not establish the responsible public body or whose technology the document discusses; it may refer to another jurisdiction.';

function publicResearchUrl(value) {
  if (typeof value !== 'string' || value.length > 4000) throw new Error('Invalid research source URL');
  let url;
  try { url = new URL(value); } catch { throw new Error('Invalid research source URL'); }
  if (url.protocol !== 'https:' || !url.hostname.includes('.') || url.username || url.password || url.hash ||
      url.port || /^(?:localhost|\[|[0-9.]+$)/i.test(url.hostname) || /\.(?:localhost|local|internal)$/i.test(url.hostname)) {
    throw new Error('Unsafe research source URL');
  }
  return url;
}

// A county-hosted link alone does not identify the body that made a decision
// or the jurisdiction whose technology is discussed in the linked document.
export function researchReferenceScope(item) {
  if (item.kind === 'meeting_document' && item.jurisdiction_ref.startsWith('us-co-county-')) {
    return {
      heading: `${item.jurisdiction} · county-linked source`,
      notice: COUNTY_DOCUMENT_SCOPE_NOTICE,
    };
  }
  return { heading: item.jurisdiction, notice: '' };
}

export function matchesResearchQuery(item, value) {
  const query = value.trim().toLocaleLowerCase();
  const people = [
    ...(item.people_context ?? []).map((person) => person.name),
    ...(item.actions ?? []).flatMap((action) => action.votes.map((vote) => vote.name)),
  ];
  return `${item.jurisdiction} ${item.file} ${item.title} ${item.signals.join(' ')} ${people.join(' ')}`.toLocaleLowerCase().includes(query);
}

function validatePortrait(portrait, context = null) {
  if (!onlyKeys(portrait, ['src', 'sha256', 'source_url', 'image_url', 'caption', 'portrait_date',
      'source_kind', 'source_document_sha256', 'source_page', 'embedded_image_index',
      'embedded_original_sha256', 'image_transform', 'identity_source_url', 'identity_agenda_url', 'identity_agenda_sha256',
      'portrait_source_sha256', 'identity_source_sha256', 'identity_matter_url', 'identity_matter_sha256']) ||
      !digest(portrait.sha256) ||
      !new RegExp(`^\\./research-portraits/${portrait.sha256}\\.(?:jpg|png|webp)$`).test(portrait.src) ||
      portrait.portrait_date !== 'not_established' || !safeText(portrait.caption, 250)) throw new Error('Invalid research portrait');
  const crossAgencyProof = ['identity_agenda_url', 'identity_agenda_sha256', 'portrait_source_sha256', 'identity_source_sha256', 'identity_matter_url', 'identity_matter_sha256'].some((key) => portrait[key] !== undefined);
  if (crossAgencyProof && (portrait.source_kind !== undefined || context?.client !== 'commerce' ||
      context.jurisdiction !== 'us-co-place-0816495' || context.personId !== 637 || context.body !== 'Urban Renewal Authority')) {
    throw new Error('Unexpected cross-agency portrait proof');
  }
  if (portrait.identity_source_url !== undefined && !(
      (context?.client === 'arapahoe' && context.jurisdiction === 'us-co-county-08005' && context.personId === 572) ||
      (context?.client === 'denver' && context.jurisdiction === 'us-co-place-0820000' && context.personId === 282) ||
      (context?.client === 'commerce' && context.jurisdiction === 'us-co-place-0816495' && context.personId === 637 && context.body === 'Urban Renewal Authority'))) {
    throw new Error('Unexpected portrait identity source');
  }
  let page;
  try { page = new URL(portrait.source_url); }
  catch { throw new Error('Invalid portrait source URL'); }
  if (page.protocol !== 'https:' ||
      !page.hostname.includes('.') || /^(?:localhost|\[|[0-9.]+$)/i.test(page.hostname) ||
      /\.(?:localhost|local|internal)$/i.test(page.hostname) ||
      page.username || page.password || page.port || page.hash) {
    throw new Error('Unsafe portrait source URL');
  }
  if (portrait.source_kind === 'embedded_pdf') {
    if (context?.client === 'denver' && context.jurisdiction === 'us-co-place-0820000' && context.personId === 472) {
      const expected = 'd81c9f4e4d3af48a6629196c5693a8a8a6b3f2a2362336a4c46c1994e68c771e';
      if (context.name !== 'Candi CdeBaca' || portrait.embedded_image_index !== 14 || portrait.sha256 !== expected ||
          portrait.src !== `./research-portraits/${expected}.png` ||
          portrait.caption !== '2020 Denver budget; source labels this portrait Candi CdeBaca.' ||
          portrait.image_url !== undefined || portrait.source_page !== 6 ||
          portrait.embedded_original_sha256 !== 'ae31c6ea93f266454d497e25c390bc4c8e34a09a1df9fcab06cb97e53086d9d0' ||
          portrait.image_transform !== 'lossless_png_from_jpeg2000' ||
          portrait.source_document_sha256 !== 'dae887ea080560ece3eae5430dbe80918b2befc548b3aa291f480acdbfe6e7e1' ||
          portrait.source_url !== 'https://www.denvergov.org/content/dam/denvergov/Portals/344/documents/Budget/2020/2020BudgetVolume1.pdf') {
        throw new Error('Embedded portrait does not match the recorded voter and source');
      }
      return;
    }
    if (context?.client === 'denver' && context.jurisdiction === 'us-co-place-0820000') {
      const expected = new Map([
        [280, ['Jolon Clark', 0, '0727c6f5be8eac62e77cebc8205ef0cce1ce135ff97660a4b2c56203a9c11cab', 'Jolon Clark, President']],
        [251, ['Rafael Espinoza', 2, '65cd497bae1eb0dc690861a35a677d5c31617e0f89a4a983ada3ea1c4ac7c368', 'Rafael Espinoza']],
        [267, ['Paul López', 4, '93678a634b6f7aadd1856e910dd779da41e472349b9e5129027375bc26b9e109', 'Paul D. López']],
        [270, ['Kendra Black', 5, '0702ffac28e24b926c78f7742e22f9716287475c2bed4cc4c4c981d4385c92a5', 'Kendra Black']],
        [275, ['Mary Beth Susman', 6, '39b60a76c699fe3cca797fac5fc740264cdc6acbfd8f083992f36486ef81ecde', 'Mary Beth Susman']],
        [282, ['Chris Herndon', 8, 'b1fee6543bec39b61756f6fae16d526678bfa52d1cfadafe48729f5ce99a5f40', 'Christopher Herndon', 'https://herndon-leadership.squarespace.com/meet-chris']],
        [286, ['Albus Brooks', 9, '574e42284d797c38e762b4d0610275abcb5391b412667182bbc2ddebc9dbbbc6', 'Albus Brooks']],
        [254, ['Wayne New', 10, '9f5ae85940f49171d75261367ae50d5b07960473436568268e0aa9f32ff9e52f', 'Wayne New']],
        [260, ['Robin Kniech', 11, '127567b9dcda9d28b615bd8af7f84889fbf13ceb4153e13920012c74631c9e30', 'Robin Kniech']],
        [263, ['Debbie Ortega', 13, '3743068bf74365a5601edae654256ee441890ad3755bbabd57fdb4b950f5a2c3', 'Deborah "Debbie" Ortega']],
      ]).get(context.personId);
      if (!expected || context.name !== expected[0] || portrait.embedded_image_index !== expected[1] ||
          portrait.sha256 !== expected[2] || portrait.src !== `./research-portraits/${expected[2]}.jpg` ||
          portrait.caption !== `2019 Denver budget; source labels this portrait ${expected[3]}.` ||
          portrait.identity_source_url !== expected[4] || portrait.image_url !== undefined || portrait.embedded_original_sha256 !== undefined ||
          portrait.image_transform !== undefined || portrait.source_page !== 6 ||
          portrait.source_document_sha256 !== '95c42fc5f567b3e4527166ff502b2e6145fce0a6bf5f8949d4df8ebc46b03fa2' ||
          portrait.source_url !== 'https://www.denvergov.org/content/dam/denvergov/Portals/344/documents/Budget/2019/2019_Budget%20Book_V1-OnlineVersion-compressed.pdf') {
        throw new Error('Embedded portrait does not match the recorded voter and source');
      }
      return;
    }
    if (context?.client === 'commerce' && context.jurisdiction === 'us-co-place-0816495') {
      const expected = new Map([
        [402, ['Benjamin Huseman', 0, '8e5dcdeeb2c2f3888626d2e57ac2a9ae14ef69e688ea546ce77164f13683ee1e']],
        [401, ['Oscar Madera', 5, 'd2f23f0633479fb607b05d60f155e00ce166afef4adecee7535314be1821b9f5']],
        [491, ['Craig Hurst', 1, 'cd742e3f608e141afa7caafa5becd9df356b3df1c7b8c0184a709525bc8e5bb6']],
        [201, ['Sean Ford', 7, '9f4489f00c0a87dab7be3a6a92bc5f2a9786cd64735f57316fd1502e134000fb']],
        [239, ['Kristi Douglas', 4, 'adc50f6895df1c0cd7e19743922c2433349feda39c8bda6c3219f5cf4127b9e2']],
      ]).get(context.personId);
      if (!expected || context.name !== expected[0] || portrait.embedded_image_index !== expected[1] ||
          portrait.sha256 !== expected[2] || portrait.src !== `./research-portraits/${expected[2]}.jpg` ||
          portrait.caption !== `2022 city council; source labels this portrait ${expected[0]}.` ||
          portrait.image_url !== undefined || portrait.embedded_original_sha256 !== undefined ||
          portrait.image_transform !== undefined || portrait.source_page !== 2 ||
          portrait.source_document_sha256 !== '142eb3b0f056a0a6d4fd82113052e5c0b081d800e76a9507c926a877bdb510be' ||
          portrait.source_url !== 'https://www.c3gov.com/files/assets/city/v/1/fd-finance-department/documents/popular-annual-financial-report/22commpafrbookletr5.pdf') {
        throw new Error('Embedded portrait does not match the recorded voter and source');
      }
      return;
    }
    if (context?.client === 'littletongov' && context.jurisdiction === 'us-co-place-0845255') {
      if ([502, 503].includes(context.personId)) {
        const later = new Map([
          [503, ['Andrea Peters', 2, 'cc93be470bce7757dad036ca4316b13818a4094cde002376b7e3aacffb2cf701']],
          [502, ['Robert Reichardt', 3, 'b154471db023063b8c63b7d4cefa0dfb55c0772e2e5e61ce35913a0ca098eee6']],
        ]).get(context.personId);
        if (!later || context.name !== later[0] || portrait.embedded_image_index !== later[1] ||
            portrait.sha256 !== later[2] || portrait.src !== `./research-portraits/${later[2]}.jpg` ||
            portrait.embedded_original_sha256 !== undefined || portrait.image_transform !== undefined ||
            portrait.caption !== `2024 city council; source labels this portrait ${later[0]}.` ||
            portrait.image_url !== undefined || portrait.source_page !== 3 ||
            portrait.source_document_sha256 !== 'f8264f7fc25a40afdc6c34ba41ea1864bbc5ac219f2485f91bf2c6ae9ba7e394' ||
            portrait.source_url !== 'https://www.littletonco.gov/files/sharedassets/public/v/1/communications/littleton-reports/2024-littleton-reports/2024-12-dec-jan-littleton-report.pdf') {
          throw new Error('Embedded portrait does not match the recorded voter and source');
        }
        return;
      }
      const identities = new Map([
        [333, ['Kyle Schlachter', 4, '4c8e72d6b9a00294f00614e0d030fc82623036e02d15d0afb203ed01c9f72d13', '0ae1bb1e3a43bf42fc30b6c05f11e718db02a7cb0f214888b8f61f3ad50b49b1']],
        [434, ['Stephen Barr', 8, 'fb3d52ef289ce7f52fd4b64a7d1783357cbb25e516fd0c7a57a9a95be0414d7e', '3c58613ff481ebfbe0009eab2c7642c2c28deac56104643532320878316966ca']],
        [352, ['Patrick Driscoll', 6, '11953d17e0c19a435d314b2780e677d31195e673cb313d4009fb534861561347', '2909cc283b189e99e31eb9b6e1de68d416910b1f00c7039cd1f3e40af1e9b750']],
        [228, ['Pam Grove', 5, '85f19484eba25681fa2d6bc75ba784c7dbbc330f614ee205bd99179e71063620', '9c8ae4afeecfd4a01d66e582cffe3ec5b7eb59bba13aace90455df181f97330d']],
        [406, ['Kelly Milliman', 2, 'be0781d7095f3e6aa71984bb76765475136adf540b8c19db323c119a992f02b7', '889170a57533c8ac0339b47fbbcdca6c787667f79e1ddcad31be3c00f9b21224']],
        [433, ['Gretchen Rydin', 7, 'e48e2401ecefc1c3c875c40681d70c595c42913bc70cb189895faaf0e9c9a628', 'b3bd9b0e108b7a84e4d261e2c0a1547212031f6314a3ec71e0e41899682e88ba']],
        [186, ['Jerry Valdes', 3, '5a6bda61589fcdfb906111edf0a9f5ebad7dbd67f6cad9c0f8ed38483253ef58', '97f2a02aa35185be8d8c204ca849047324cc5f917442a53aef39c19f1abc3c1a']],
      ]);
      const expected = identities.get(context.personId);
      if (!expected || context.name !== expected[0] || portrait.embedded_image_index !== expected[1] ||
          portrait.sha256 !== expected[2] || portrait.embedded_original_sha256 !== expected[3] ||
          portrait.src !== `./research-portraits/${expected[2]}.png` ||
          portrait.image_transform !== 'lossless_png_from_jpeg2000' ||
          portrait.caption !== `2023 city council; source labels this portrait ${expected[0]}.` ||
          portrait.image_url !== undefined || portrait.source_page !== 3 ||
          portrait.source_document_sha256 !== '34a8dc30a20a37c23179b122722175be6cf3c253c825009db6fbdeb4cff6ea7e' ||
          portrait.source_url !== 'https://www.littletonco.gov/files/sharedassets/public/v/1/communications/littleton-reports/2023-littleton-reports/2023-02-feb-littleton-report.pdf') {
        throw new Error('Embedded portrait does not match the recorded voter and source');
      }
      return;
    }
    if (context?.client === 'arapahoe' && context.jurisdiction === 'us-co-county-08005' && [571, 572].includes(context.personId)) {
      const expected = new Map([
        [572, ['Jessica Campbell', 1, '3ea3f82d546b3de7b1d07cfdb69e429e643b6ade00677f189bdee6fc59b1e5a6', 'Jessica Campbell-Swanson', 'https://www.jessicaforcommissioner.com/home-1']],
        [571, ['Leslie Summey', 3, '942839934eb8ea112842148a95aa7deb38f94793e503d7ea5bf76e09cf439da2', 'Leslie Summey', undefined]],
      ]).get(context.personId);
      if (!expected || context.name !== expected[0] || portrait.embedded_image_index !== expected[1] ||
          portrait.sha256 !== expected[2] || portrait.src !== `./research-portraits/${expected[2]}.jpg` ||
          portrait.caption !== `2024 county board; source labels this portrait ${expected[3]}.` ||
          portrait.identity_source_url !== expected[4] || portrait.image_url !== undefined ||
          portrait.embedded_original_sha256 !== undefined || portrait.image_transform !== undefined ||
          portrait.source_page !== 61 || portrait.source_document_sha256 !== '4e5366320ab2e190c4f5ab080669458566ed7a82fc666da85330db7588528f0c' ||
          portrait.source_url !== 'https://cms3.revize.com/revize/arapahoe/Finance/Complete%20Budget%20Document/2024%20Adopted%20Budget.pdf') {
        throw new Error('Embedded portrait does not match the recorded voter and source');
      }
      return;
    }
    const identities = new Map([
      [513, ['Carrie Warren-Gully', 6, '84202caa2919b815df89984e12ad15f4de4289a35b74f048f54f0e68f0954174', 'Carrie Warren Gully']],
      [225, ['Nancy Sharpe', 7, '6555e2248f94d78cd16f6f4688f61c69da5029755f3da2ed9a982f6639588a8e', 'Nancy N. Sharpe']],
      [226, ['Jeff Baker', 8, '62aa905c15672fba1f48abd5a356e03dda7f2492077dab2a77e4543ae9063314', 'Jeff Baker']],
      [227, ['Nancy Jackson', 9, '50405246a18ce227b0671befb2a0e5148e83d3e54acdec20d3e3d1cf0c2122da', 'Nancy Jackson']],
      [228, ['Bill Holen', 10, '6023e0418ecc3a5f92ee2523482d6941de9156fd567cf46f3e9c6dd25370516e', 'Bill L. Holen']],
    ]);
    const expected = identities.get(context?.personId);
    if (!expected || context.client !== 'arapahoe' || context.jurisdiction !== 'us-co-county-08005' ||
        context.name !== expected[0] || portrait.embedded_image_index !== expected[1] || portrait.sha256 !== expected[2] ||
        portrait.caption !== `2022 county board; source labels this portrait ${expected[3]}.` ||
        portrait.image_url !== undefined || portrait.source_page !== 7 ||
        portrait.embedded_original_sha256 !== undefined || portrait.image_transform !== undefined ||
        portrait.src !== `./research-portraits/${expected[2]}.jpg` ||
        portrait.source_document_sha256 !== 'dd6476e64941674e310110b375d90dcf9f81ef904f0866183337b2b0fb867c43' ||
        portrait.source_url !== 'https://cms3.revize.com/revize/arapahoe/Finance/Complete%20Budget%20Document/2023%20Recommended%20Budget.pdf') {
      throw new Error('Embedded portrait does not match the recorded voter and source');
    }
    return;
  }
  if (portrait.source_kind !== undefined || portrait.source_document_sha256 !== undefined ||
      portrait.source_page !== undefined || portrait.embedded_image_index !== undefined ||
      portrait.embedded_original_sha256 !== undefined || portrait.image_transform !== undefined) throw new Error('Unknown portrait source kind');
  if (crossAgencyProof) {
    if (context?.client !== 'commerce' || context.jurisdiction !== 'us-co-place-0816495' || context.personId !== 637 ||
        context.name !== 'Craig Machuga' || context.body !== 'Urban Renewal Authority' ||
        portrait.source_url !== 'https://www.sacfd.org/meet-our-district-board-of-directors' ||
        portrait.identity_source_url !== 'https://www.c3gov.com/Government/Boards-Commissions/Urban-Renewal-Authority' ||
        portrait.identity_agenda_url !== 'https://ompnetwork.s3-us-west-2.amazonaws.com/sites/1717/documents/agenda_22_1.pdf' ||
        portrait.identity_agenda_sha256 !== '2936f9b6bee4c0a1b1556579dd8fb807fa0bfc5a7ec357e1fb22e352ec39a67a' ||
        portrait.portrait_source_sha256 !== 'c62be165e986063ecd8b955dd3ee730516f92268d823060cc1d40d2abd79609b' ||
        portrait.identity_source_sha256 !== 'b7f94b19ff9a65699174df106a141fb10d3a2328b4dc3e6cd64b640be6423403' ||
        portrait.identity_matter_url !== 'https://webapi.legistar.com/v1/commerce/Matters/9151' ||
        portrait.identity_matter_sha256 !== 'a73a53441b86e65b3a81080f34dfe76563c203a9530d65a2a0ea7ff87741358f' ||
        portrait.sha256 !== '03afcbdf3dcadad64a2817377403fc0e49cd55a9b6ec6855b4e78f33524e5a3f' ||
        portrait.caption !== 'Fire board name-labeled card shows Craig Machuga; page 1 of the 2022 appointment agenda lists a South Adams Fire Department recommendation. Identity connection is preliminary.' ||
        portrait.image_url !== 'https://streamline.imgix.net/0ec9c10d-3273-4b87-a0de-62fc6b0603cd/9a0b203c-7e23-4e21-8628-e6390fbdd0c9/C.%20Machuga.JPG?ixlib=rb-1.1.0&or=0&w=720&h=720&fit=fill&fill=blur&auto=format%2Ccompress&s=066395a71136c9612a133dc915584835') throw new Error('Invalid cross-agency portrait proof');
    return;
  }
  let image;
  try { image = new URL(portrait.image_url); }
  catch { throw new Error('Invalid portrait image URL'); }
  if (image.protocol !== 'https:' || page.hostname !== image.hostname || image.username || image.password || image.port || image.hash) {
    throw new Error('Unsafe portrait image URL');
  }
}

export function validResearchJurisdiction(value) {
  return typeof value === 'string' && /^(?:us-co-[a-z0-9-]{1,100}|dola-district-[0-9]{5})$/.test(value);
}

export function validateResearch(report) {
  if (!onlyKeys(report, ['schema_version', 'state', 'generated_at', 'notice', 'coverage', 'records']) ||
      report.schema_version !== 'pnull-research-v1' || report.state !== 'preliminary' ||
      !Array.isArray(report.records) || report.records.length > 50000 ||
      !onlyKeys(report.coverage, ['lookback_period', 'jurisdictions_with_mentions', 'matter_references',
        'document_references', 'agenda_portal_references', 'official_page_references', 'agenda_portal_screening', 'search_attempts', 'recorded_actions', 'recorded_vote_entries', 'document_screening',
        'refresh_failures_preserving_prior_results', 'non_surveillance_mentions_withheld', 'omitted_failed_provenance', 'ocr_revalidation_pending']) ||
      report.coverage.lookback_period !== null ||
      !onlyKeys(report.coverage.document_screening, ['pending', 'error', 'matched', 'no_match', 'redirected']) ||
      !Object.values(report.coverage.document_screening).every(nonnegative) ||
      (report.coverage.ocr_revalidation_pending !== undefined &&
        (!nonnegative(report.coverage.ocr_revalidation_pending) || report.coverage.ocr_revalidation_pending > (report.coverage.document_screening.error ?? 0))) ||
      (report.coverage.agenda_portal_screening !== undefined &&
        (!onlyKeys(report.coverage.agenda_portal_screening, ['pending', 'error', 'matched', 'no_match']) ||
          !Object.values(report.coverage.agenda_portal_screening).every(nonnegative))) ||
      !nonnegative(report.coverage.omitted_failed_provenance) ||
      (report.coverage.non_surveillance_mentions_withheld !== undefined &&
        !nonnegative(report.coverage.non_surveillance_mentions_withheld)) ||
      (report.coverage.refresh_failures_preserving_prior_results !== undefined &&
        !nonnegative(report.coverage.refresh_failures_preserving_prior_results)) ||
      typeof report.notice !== 'string' || report.notice.length > 500 ||
      /[\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069]/u.test(report.notice) ||
      typeof report.generated_at !== 'string' || !Number.isFinite(Date.parse(report.generated_at))) {
    throw new Error('Invalid preliminary research report');
  }
  const seen = new Set();
  if (report.coverage.search_attempts !== undefined) {
    if (!Array.isArray(report.coverage.search_attempts) || report.coverage.search_attempts.length > 5000) throw new Error('Invalid research attempts');
    const places = new Set();
    for (const attempt of report.coverage.search_attempts) {
      if (!onlyKeys(attempt, ['jurisdiction_ref', 'checked_at', 'website_check', 'documents_screened', 'documents_failed',
        'documents_queued', 'documents_redirected', 'ocr_revalidation_pending', 'portal_pages_checked', 'historical_index_pages_checked', 'historical_index_errors', 'agenda_meeting_pages_screened', 'lookback_period']) ||
          !validResearchJurisdiction(attempt.jurisdiction_ref) || places.has(attempt.jurisdiction_ref) ||
          typeof attempt.checked_at !== 'string' || !Number.isFinite(Date.parse(attempt.checked_at)) ||
          !['site_checked', 'unavailable'].includes(attempt.website_check) || attempt.lookback_period !== null ||
          ![attempt.documents_screened, attempt.documents_failed, attempt.documents_queued, attempt.portal_pages_checked,
            attempt.agenda_meeting_pages_screened].every(nonnegative) ||
          (attempt.historical_index_pages_checked !== undefined && !nonnegative(attempt.historical_index_pages_checked)) ||
          (attempt.documents_redirected !== undefined && !nonnegative(attempt.documents_redirected)) ||
          (attempt.historical_index_errors !== undefined && !nonnegative(attempt.historical_index_errors)) ||
          (attempt.ocr_revalidation_pending !== undefined && (!nonnegative(attempt.ocr_revalidation_pending) ||
            attempt.ocr_revalidation_pending > attempt.documents_failed))) throw new Error('Invalid research attempt');
      places.add(attempt.jurisdiction_ref);
    }
  }
  for (const record of report.records) {
    if (!onlyKeys(record, ['id', 'kind', 'confidence', 'jurisdiction_ref', 'jurisdiction', 'file', 'title', 'signals',
      'source_status', 'source_url', 'matter_url', 'source_sha256', 'locator', 'captured_at', 'last_screened_at',
      'people_context', 'historical', 'ocr', 'actions', 'document_format',
      'capture_adjustment', 'indexed_source_url', 'source_index_url', 'document_highlights', 'springs_source_chain', 'context_state']) ||
        !/^co-[a-z0-9-]{1,150}$/.test(record.id) || seen.has(record.id) ||
        !['matter_reference', 'meeting_document', 'agenda_portal_reference', 'official_page_context'].includes(record.kind) ||
        record.confidence !== 'unconfirmed_source_reference' ||
        !validResearchJurisdiction(record.jurisdiction_ref) ||
        typeof record.jurisdiction !== 'string' || !record.jurisdiction.trim() || record.jurisdiction.length > 300 ||
        typeof record.title !== 'string' || record.title.length > 400 ||
        typeof record.file !== 'string' || record.file.length > 120 ||
        typeof record.source_status !== 'string' || record.source_status.length > 100 ||
        !Array.isArray(record.signals) || !record.signals.length || record.signals.length > 24 ||
        record.signals.some((item) => typeof item !== 'string' || !item.trim() || item.length > 100) ||
        !/^[0-9a-f]{64}$/.test(record.source_sha256) ||
        !/^(?:\$\[\d+\]\.MatterTitle|extracted line [1-9]\d*|HTML text node [1-9]\d*|Source response \(withdrawal\))$/.test(record.locator) ||
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
      const url = publicResearchUrl(value);
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
    } else if (record.kind === 'meeting_document' && (!/^co-document-[a-f0-9]{24}$/.test(record.id) ||
        (parsedUrls[0].search && (!/^\?t=[0-9]{1,20}$/.test(parsedUrls[0].search) || !/\.(?:pdf|docx)$/i.test(parsedUrls[0].pathname))) ||
        !(/\/AgendaCenter\/ViewFile\/(?:Agenda|Minutes)\/_[0-9]{8}-[0-9]{1,12}$/i.test(parsedUrls[0].pathname) ||
          /\.(?:pdf|docx)$/i.test(parsedUrls[0].pathname)))) throw new Error('Meeting document URL is inconsistent');
    else if (record.kind === 'agenda_portal_reference') {
      const meeting = /^\/iip\/elpaso\/meeting\/details\/([1-9]\d*)$/.exec(parsedUrls[0].pathname);
      if (parsedUrls[0].hostname !== 'www.agendasuite.org' || parsedUrls[0].search || !meeting ||
          record.jurisdiction_ref !== 'us-co-county-08041' || record.id !== `co-agendasuite-elpaso-${meeting[1]}-${record.source_sha256.slice(0, 12)}` ||
          !/^HTML text node [1-9]\d*$/.test(record.locator)) throw new Error('Agenda portal source is inconsistent');
    }
    else if (record.kind === 'official_page_context') {
      // BEGIN GENERATED OFFICIAL CONTEXT ALLOWLIST
      const allowed = {
        "co-official-context-boulder-flock": ["us-co-place-0807850", "Boulder", "https://bouldercolorado.gov/services/flock-safety-cameras-and-boulder-police-department", "Boulder police describe their selection of Flock Safety.", ["Flock Safety"]],
        "co-official-context-boulder-uas": ["us-co-place-0807850", "Boulder", "https://bouldercolorado.gov/services/unmanned-aircraft-system", "Boulder police describe their unmanned-aircraft program.", ["Police drone"]],
        "co-official-context-broomfield-flock": ["us-co-place-0809280", "Broomfield", "https://www.broomfield.org/4545/Flock-Safety-License-Plate-Readers", "Broomfield police describe their Flock Safety license-plate-reader program.", ["Flock Safety", "Automated License Plate Readers"]],
        "co-official-context-greeley-flock": ["us-co-place-0832155", "Greeley", "https://greeleyco.gov/police/technology/automated-license-plate-reader-program/", "Greeley police describe their Flock Safety camera program.", ["Flock Safety"]],
        "co-official-context-sterling-bwc": ["us-co-place-0873935", "Sterling", "https://www.sterlingcolo.com/departments/police_department/officer_body-worn_cameras.php", "Sterling police describe their body-worn camera program.", ["Body-worn cameras"]],
        "co-official-context-westminster-axon": ["us-co-place-0883835", "Westminster", "https://www.westminsterco.gov/180/Engagement-Transparency", "Westminster police describe their Axon body-worn-camera program.", ["Axon", "Body-worn cameras"]],
      }[record.id];
      // END GENERATED OFFICIAL CONTEXT ALLOWLIST
      const withdrawn = record.context_state === 'withdrawn';
      if (!allowed || !['active', 'withdrawn'].includes(record.context_state) || record.jurisdiction_ref !== allowed[0] || record.jurisdiction !== allowed[1] ||
          record.source_url !== allowed[2] || record.title !== (withdrawn ? 'Official agency page changed; prior technology context withdrawn.' : allowed[3]) ||
          JSON.stringify(record.signals) !== JSON.stringify(withdrawn ? ['Source changed'] : allowed[4]) ||
          record.file !== 'Official police technology page' || record.source_status !== '' ||
          (withdrawn ? record.locator !== 'Source response (withdrawal)' : !/^HTML text node [1-9]\d*$/.test(record.locator)) || record.actions !== undefined ||
          record.people_context !== undefined || record.ocr !== undefined || record.historical !== undefined) {
        throw new Error('Official context identity is inconsistent');
      }
    }
    if (record.kind !== 'official_page_context' && (record.context_state !== undefined || record.locator === 'Source response (withdrawal)')) {
      throw new Error('Unexpected official context state');
    }
    if (record.kind !== 'matter_reference' && record.matter_url !== undefined) throw new Error('Unexpected matter URL');
    if (record.document_highlights !== undefined) {
      const seenLocations = new Set([record.locator]);
      if (record.kind !== 'meeting_document' || record.ocr === true || !Array.isArray(record.document_highlights) ||
          !record.document_highlights.length || record.document_highlights.length > 5) throw new Error('Invalid document highlights');
      for (const highlight of record.document_highlights) {
        if (!onlyKeys(highlight, ['locator', 'title', 'signals']) ||
            !/^extracted line [1-9]\d*$/.test(highlight.locator) || seenLocations.has(highlight.locator) ||
            !safeText(highlight.title, 400) || !highlight.title.trim() || !Array.isArray(highlight.signals) ||
            !highlight.signals.length || highlight.signals.length > 24 ||
            highlight.signals.some((signal) => !safeText(signal, 100) || !signal.trim())) throw new Error('Invalid document highlight');
        seenLocations.add(highlight.locator);
      }
    }
    if (record.springs_source_chain !== undefined) {
      const chain = record.springs_source_chain;
      if (record.kind !== 'meeting_document' || record.jurisdiction_ref !== 'us-co-place-0816000' ||
          record.capture_adjustment !== undefined || !onlyKeys(chain, ['original_index_url', 'original_index_sha256',
            'original_anchor_index', 'original_document_url', 'redirect_status', 'redirect_captured_at',
            'redirect_receipt_sha256', 'landing_url', 'landing_sha256', 'landing_anchor_index']) ||
          !digest(chain.original_index_sha256) || !digest(chain.redirect_receipt_sha256) || !digest(chain.landing_sha256) ||
          ![chain.original_anchor_index, chain.landing_anchor_index].every((n) => Number.isSafeInteger(n) && n >= 0 && n < 100000) ||
          ![301, 302, 303, 307, 308].includes(chain.redirect_status) ||
          typeof chain.redirect_captured_at !== 'string' || !Number.isFinite(Date.parse(chain.redirect_captured_at))) {
        throw new Error('Invalid Colorado Springs source chain');
      }
      const index = publicResearchUrl(chain.original_index_url);
      const original = publicResearchUrl(chain.original_document_url);
      const landing = publicResearchUrl(chain.landing_url);
      const capture = parsedUrls[0];
      const old = /^\/sites\/default\/files\/(?:inline-images\/)?([A-Za-z0-9._-]+\.pdf)$/i.exec(original.pathname);
      const dest = /^\/document\/([A-Za-z0-9.-]+\.pdf)$/i.exec(landing.pathname);
      if ([index, original, landing, capture].some((u) => u.hostname !== 'coloradosprings.gov') ||
          (index.search && !/^\?page=[0-9]{1,5}$/.test(index.search)) || original.search || landing.search || capture.search ||
          !old || !dest || old[1].includes('..') || dest[1].includes('..') ||
          old[1].replaceAll('_', '').toLowerCase() !== dest[1].toLowerCase() ||
          capture.pathname !== '/system' + original.pathname.slice('/sites/default'.length)) {
        throw new Error('Colorado Springs source chain identity differs');
      }
    }
    const hasRedirectProof = record.capture_adjustment !== undefined || record.indexed_source_url !== undefined || record.source_index_url !== undefined;
    if (hasRedirectProof) {
      if (record.kind !== 'meeting_document' || !['timestamp_omitted', 'direct_redirect'].includes(record.capture_adjustment)) {
        throw new Error('Invalid alternate document capture');
      }
      const linked = publicResearchUrl(record.indexed_source_url);
      const index = publicResearchUrl(record.source_index_url);
      const capture = parsedUrls[0];
      const prefix = capture.pathname.endsWith(linked.pathname) ? capture.pathname.slice(0, -linked.pathname.length) : '';
      const revize = linked.hostname === index.hostname && !capture.search &&
        /^(?:cms[0-9]+(?:files[0-9]*)?|webgen[0-9]+files[0-9]*)\.revize\.com$/i.test(capture.hostname) &&
        /^\/(?:revize\/)?[a-z0-9_-]{1,80}$/i.test(prefix) &&
        (record.capture_adjustment === 'timestamp_omitted' ? /^\?t=[0-9]{1,20}$/.test(linked.search) : !linked.search);
      const tenant = /^\/go\/([a-z0-9][a-z0-9-]{0,79})\/agendas\/[1-9][0-9]{0,11}\/PDF\/get_agenda_document_link$/.exec(linked.pathname)?.[1];
      const towncloud = record.capture_adjustment === 'direct_redirect' && tenant &&
        linked.hostname === 'towncloud.io' && index.hostname !== linked.hostname && !linked.search &&
        capture.hostname === 'towncloud-core-prod.s3.us-west-2.amazonaws.com' && !capture.search &&
        new RegExp(`^/uploads/${tenant}/meetings/agenda_packet/pdf/[1-9][0-9]{0,11}/[A-Za-z0-9._~-]{1,180}\\.pdf$`).test(capture.pathname);
      if (!revize && !towncloud) {
        throw new Error('Alternate document does not match its indexed source');
      }
    }
    if (record.kind === 'meeting_document' && record.ocr !== undefined && typeof record.ocr !== 'boolean') throw new Error('Invalid OCR label');
    if (record.document_format !== undefined && (record.kind !== 'meeting_document' || !['pdf', 'docx'].includes(record.document_format))) throw new Error('Invalid document format');
    if (record.kind === 'meeting_document' && ((/\.docx$/i.test(parsedUrls[0].pathname) && record.document_format !== 'docx') ||
        (/\.pdf$/i.test(parsedUrls[0].pathname) && record.document_format === 'docx'))) throw new Error('Document format conflicts with its URL');
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
          if (vote.portrait) validatePortrait(vote.portrait, { client: matter[1], jurisdiction: record.jurisdiction_ref,
            personId: vote.person_id, name: vote.name, body: motion.body });
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
      (report.coverage.official_page_references ?? 0) !== count('official_page_context') ||
      report.coverage.jurisdictions_with_mentions !== new Set(report.records.filter((item) => item.context_state !== 'withdrawn').map((item) => item.jurisdiction_ref)).size) {
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
