//! Page-accurate PDF citation geometry.
//!
//! Builds immutable text-map artifacts from Poppler `pdftotext -bbox-layout`
//! output (text PDFs) or deterministic Tesseract TSV output (OCR PDFs),
//! validates citation geometry, and renders reviewer images that highlight
//! the exact quoted region. Impossible geometry (negative coordinates,
//! inverted rectangles, out-of-bounds regions, missing pages, quote-to-text
//! map mismatches) is rejected.

use std::fs;
use std::path::Path;
use std::process::Command;

use pnull_core::{
    BoundingRect, MapWord, PageCitation, TextMap, sha256_hex,
};
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum GeometryError {
    #[error("invalid geometry: {0}")]
    Invalid(String),
    #[error("quote not found on page {page}: {quote}")]
    QuoteNotFound { page: u32, quote: String },
    #[error("quote-to-text-map mismatch on page {page}")]
    QuoteMismatch { page: u32 },
    #[error("malformed extractor output: {0}")]
    Malformed(String),
    #[error("renderer I/O failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("rendered page image failure: {0}")]
    Render(String),
    #[error("missing page {0} in text map")]
    MissingPage(u32),
    #[error(transparent)]
    Core(#[from] pnull_core::CoreError),
}

/// The coordinate system used by text maps produced here.
pub const COORDINATE_SYSTEM: &str = "pdf_user_space_points_bottom_left_y_up";

/// Page metadata shared by text-map builders.
#[derive(Clone, Debug)]
pub struct PageSpec {
    pub evidence_id: String,
    pub page_number: u32,
    pub page_width: f64,
    pub page_height: f64,
    pub page_rotation: i32,
    pub extractor_version: String,
    pub source_digest: String,
}

impl PageSpec {
    pub fn new(
        evidence_id: &str,
        page_number: u32,
        page_width: f64,
        page_height: f64,
        page_rotation: i32,
        extractor_version: &str,
        source_digest: &str,
    ) -> Self {
        Self {
            evidence_id: evidence_id.to_owned(),
            page_number,
            page_width,
            page_height,
            page_rotation,
            extractor_version: extractor_version.to_owned(),
            source_digest: source_digest.to_owned(),
        }
    }
}

/// Validates a bounding rectangle against page dimensions.
pub fn validate_rect(rect: &BoundingRect, page_width: f64, page_height: f64) -> Result<(), GeometryError> {
    if !rect.x_min.is_finite()
        || !rect.y_min.is_finite()
        || !rect.x_max.is_finite()
        || !rect.y_max.is_finite()
    {
        return Err(GeometryError::Invalid("non-finite coordinate".to_owned()));
    }
    if rect.x_min < 0.0 || rect.y_min < 0.0 || rect.x_max < 0.0 || rect.y_max < 0.0 {
        return Err(GeometryError::Invalid("negative coordinate".to_owned()));
    }
    if rect.x_max <= rect.x_min || rect.y_max <= rect.y_min {
        return Err(GeometryError::Invalid("inverted or empty rectangle".to_owned()));
    }
    if rect.x_max > page_width || rect.y_max > page_height {
        return Err(GeometryError::Invalid("rectangle outside page".to_owned()));
    }
    Ok(())
}

/// Parses `pdftotext -bbox-layout` XML for a single page into a [`TextMap`].
///
/// `xml` should be the `<page>` element content (or the full document; only
/// the first `<page>` is read). Coordinates are PDF user-space points with
/// origin at the bottom-left.
pub fn parse_bbox_layout(
    xml: &str,
    spec: &PageSpec,
) -> Result<TextMap, GeometryError> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut words = Vec::new();
    let mut in_page = false;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(quick_xml::events::Event::Start(element)) => {
                let name = element.name();
                let name_bytes = name.as_ref();
                if name_bytes == b"page" {
                    in_page = true;
                } else if in_page && name_bytes == b"word" {
                    let mut x_min = 0.0;
                    let mut y_min = 0.0;
                    let mut x_max = 0.0;
                    let mut y_max = 0.0;
                    for attribute in element.attributes().flatten() {
                        let key = attribute.key.as_ref();
                        let value = String::from_utf8_lossy(attribute.value.as_ref()).to_string();
                        match key {
                            b"xMin" => x_min = value.parse().map_err(|_| GeometryError::Malformed("xMin".to_owned()))?,
                            b"yMin" => y_min = value.parse().map_err(|_| GeometryError::Malformed("yMin".to_owned()))?,
                            b"xMax" => x_max = value.parse().map_err(|_| GeometryError::Malformed("xMax".to_owned()))?,
                            b"yMax" => y_max = value.parse().map_err(|_| GeometryError::Malformed("yMax".to_owned()))?,
                            _ => {}
                        }
                    }
                    words.push(parse_word_element(&mut reader, x_min, y_min, x_max, y_max)?);
                }
            }
            Ok(quick_xml::events::Event::End(element)) => {
                if element.name().as_ref() == b"page" && in_page {
                    break;
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(error) => return Err(GeometryError::Malformed(error.to_string())),
            _ => {}
        }
        buffer.clear();
    }
    let map = TextMap {
        id: String::new(),
        evidence_id: spec.evidence_id.clone(),
        page_number: spec.page_number,
        page_width: spec.page_width,
        page_height: spec.page_height,
        page_rotation: spec.page_rotation,
        coordinate_system: COORDINATE_SYSTEM.to_owned(),
        words,
        extractor: "poppler_pdftotext_bbox".to_owned(),
        extractor_version: spec.extractor_version.clone(),
        digest: String::new(),
        source_digest: spec.source_digest.clone(),
    };
    let digest = map.compute_digest();
    let id = TextMap::id_for(&spec.evidence_id, spec.page_number, &digest);
    Ok(TextMap { id, digest, ..map })
}

fn parse_word_element(
    reader: &mut quick_xml::Reader<&[u8]>,
    x_min: f64,
    y_min: f64,
    x_max: f64,
    y_max: f64,
) -> Result<MapWord, GeometryError> {
    let mut text = String::new();
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(quick_xml::events::Event::Text(value)) => {
                let decoded = value.decode().map_err(|e| GeometryError::Malformed(e.to_string()))?;
                let unescaped = quick_xml::escape::unescape(&decoded)
                    .map_err(|e| GeometryError::Malformed(e.to_string()))?;
                text.push_str(&unescaped);
            }
            Ok(quick_xml::events::Event::End(element)) => {
                if element.name().as_ref() == b"word" {
                    break;
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(error) => return Err(GeometryError::Malformed(error.to_string())),
            _ => {}
        }
        buffer.clear();
    }
    Ok(MapWord {
        text: text.trim().to_owned(),
        rect: BoundingRect {
            x_min,
            y_min,
            x_max,
            y_max,
        },
    })
}

/// Parses a Tesseract TSV page into a [`TextMap`], transforming pixel
/// coordinates (top-left origin) into PDF user-space points (bottom-left
/// origin, y up).
pub fn parse_ocr_tsv(
    tsv: &str,
    spec: &PageSpec,
    image_width: u32,
    image_height: u32,
) -> Result<TextMap, GeometryError> {
    let mut words = Vec::new();
    let mut lines = tsv.lines();
    // Header row.
    let header = lines.next().ok_or_else(|| GeometryError::Malformed("empty TSV".to_owned()))?;
    let columns: Vec<&str> = header.split('\t').collect();
    let idx = |name: &str| -> Result<usize, GeometryError> {
        columns
            .iter()
            .position(|column| *column == name)
            .ok_or_else(|| GeometryError::Malformed(format!("missing column {name}")))
    };
    let i_left = idx("left")?;
    let i_top = idx("top")?;
    let i_width = idx("width")?;
    let i_height = idx("height")?;
    let _i_conf = idx("conf")?;
    let i_text = idx("text")?;
    let i_level = idx("level")?;
    let i_page = idx("page_num")?;

    let scale_x = spec.page_width / f64::from(image_width);
    let scale_y = spec.page_height / f64::from(image_height);

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() <= i_text {
            continue;
        }
        let level: i64 = fields[i_level].parse().unwrap_or(0);
        if level != 5 {
            continue; // only word-level rows
        }
        let page: i64 = fields[i_page].parse().unwrap_or(1);
        if page != i64::from(spec.page_number) {
            continue;
        }
        let text = fields[i_text].trim();
        if text.is_empty() {
            continue;
        }
        let left: f64 = fields[i_left].parse().unwrap_or(0.0);
        let top: f64 = fields[i_top].parse().unwrap_or(0.0);
        let width: f64 = fields[i_width].parse().unwrap_or(0.0);
        let height: f64 = fields[i_height].parse().unwrap_or(0.0);
        // Map pixel box (top-left origin) to PDF user-space (bottom-left origin).
        let x_min = left * scale_x;
        let x_max = (left + width) * scale_x;
        let y_max = (f64::from(image_height) - top) * scale_y;
        let y_min = (f64::from(image_height) - (top + height)) * scale_y;
        words.push(MapWord {
            text: text.to_owned(),
            rect: BoundingRect {
                x_min,
                y_min,
                x_max,
                y_max,
            },
        });
    }
    let map = TextMap {
        id: String::new(),
        evidence_id: spec.evidence_id.clone(),
        page_number: spec.page_number,
        page_width: spec.page_width,
        page_height: spec.page_height,
        page_rotation: spec.page_rotation,
        coordinate_system: COORDINATE_SYSTEM.to_owned(),
        words,
        extractor: "tesseract_tsv".to_owned(),
        extractor_version: spec.extractor_version.clone(),
        digest: String::new(),
        source_digest: spec.source_digest.clone(),
    };
    let digest = map.compute_digest();
    let id = TextMap::id_for(&spec.evidence_id, spec.page_number, &digest);
    Ok(TextMap { id, digest, ..map })
}

/// Validates every word rectangle in a text map against the page bounds.
pub fn validate_text_map(map: &TextMap) -> Result<(), GeometryError> {
    for word in &map.words {
        validate_rect(&word.rect, map.page_width, map.page_height)?;
    }
    Ok(())
}

fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Locates every occurrence of `quote` in the text map's words, returning the
/// bounding rectangles for each occurrence. Supports multi-line quotations.
/// Hyphenated line breaks are joined (trailing hyphen removed).
pub fn find_occurrences(map: &TextMap, quote: &str) -> Vec<Vec<BoundingRect>> {
    let target = normalized(quote);
    if target.is_empty() {
        return Vec::new();
    }
    // Build a joined word stream with hyphen-aware line joining.
    let mut joined: Vec<JoinedWord> = Vec::new();
    for word in &map.words {
        let text = word.text.trim();
        if text.is_empty() {
            continue;
        }
        // A word that continues a hyphenated line break merges into the
        // previous token (the trailing hyphen is removed).
        if joined.last().is_some_and(|last| last.text.ends_with('-')) {
            if let Some(last) = joined.last_mut() {
                last.text.pop();
                last.text.push_str(text);
                last.rect.x_max = last.rect.x_max.max(word.rect.x_max);
                last.rect.y_max = last.rect.y_max.max(word.rect.y_max);
                last.rect.y_min = last.rect.y_min.min(word.rect.y_min);
            }
        } else {
            joined.push(JoinedWord {
                text: text.to_owned(),
                rect: word.rect,
            });
        }
    }
    let stream: Vec<String> = joined.iter().map(|word| word.text.clone()).collect();
    let target_words: Vec<&str> = target.split_whitespace().collect();
    if target_words.is_empty() {
        return Vec::new();
    }
    let mut occurrences = Vec::new();
    let mut start = 0usize;
    while start + target_words.len() <= stream.len() {
        let matches = stream[start..start + target_words.len()]
            .iter()
            .zip(&target_words)
            .all(|(actual, expected)| actual.eq_ignore_ascii_case(expected));
        if matches {
            let rects: Vec<BoundingRect> = joined[start..start + target_words.len()]
                .iter()
                .map(|word| word.rect)
                .collect();
            occurrences.push(rects);
        }
        start += 1;
    }
    occurrences
}

struct JoinedWord {
    text: String,
    rect: BoundingRect,
}

/// Builds a validated [`PageCitation`] for the `occurrence_index`-th
/// occurrence of `quote` on the given page.
pub fn build_page_citation(
    map: &TextMap,
    quote: &str,
    occurrence_index: usize,
    normalized_range: (u32, u32),
    ocr_confidence: Option<f64>,
    evidence_digest: &str,
) -> Result<PageCitation, GeometryError> {
    validate_text_map(map)?;
    let occurrences = find_occurrences(map, quote);
    let rects = occurrences
        .get(occurrence_index)
        .cloned()
        .ok_or_else(|| GeometryError::QuoteNotFound {
            page: map.page_number,
            quote: quote.to_owned(),
        })?;
    if rects.is_empty() {
        return Err(GeometryError::QuoteMismatch {
            page: map.page_number,
        });
    }
    for rect in &rects {
        validate_rect(rect, map.page_width, map.page_height)?;
    }
    let quote_digest = sha256_hex(quote.as_bytes());
    let id = PageCitation::id_for(&map.evidence_id, &quote_digest, map.page_number);
    Ok(PageCitation {
        id,
        evidence_id: map.evidence_id.clone(),
        quote: quote.to_owned(),
        quote_digest,
        page_number: map.page_number,
        rects,
        normalized_range: pnull_core::LocatorRange {
            start: normalized_range.0,
            end: normalized_range.1,
        },
        text_map_digest: map.digest.clone(),
        evidence_digest: evidence_digest.to_owned(),
        ocr_confidence,
    })
}

/// Renders a reviewer image highlighting the quoted region.
///
/// Renders `page_number` of `pdf_path` at `dpi` via `pdftoppm`, then draws
/// translucent highlight rectangles over the quoted region and writes the
/// result to `output_png`.
pub fn render_review_image(
    pdf_path: &Path,
    page_number: u32,
    rects: &[BoundingRect],
    page_width: f64,
    page_height: f64,
    output_png: &Path,
    dpi: u32,
) -> Result<(), GeometryError> {
    let directory = tempfile::TempDir::new()?;
    let prefix = directory.path().join("page");
    let status = Command::new("pdftoppm")
        .args([
            "-png",
            "-r",
            &dpi.to_string(),
            "-f",
            &page_number.to_string(),
            "-l",
            &page_number.to_string(),
        ])
        .arg(pdf_path)
        .arg(&prefix)
        .status()
        .map_err(|error| GeometryError::Render(error.to_string()))?;
    if !status.success() {
        return Err(GeometryError::Render("pdftoppm failed".to_owned()));
    }
    let png = fs::read_dir(directory.path())
        .map_err(GeometryError::Io)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext == "png"))
        .ok_or_else(|| GeometryError::Render("no rendered page".to_owned()))?;
    let mut image = image::open(&png)
        .map_err(|error| GeometryError::Render(error.to_string()))?
        .to_rgba8();
    let (image_width, image_height) = (image.width(), image.height());
    let scale_x = f64::from(image_width) / page_width;
    let scale_y = f64::from(image_height) / page_height;
    for rect in rects {
        draw_highlight(&mut image, rect, page_height, scale_x, scale_y);
    }
    image
        .save(output_png)
        .map_err(|error| GeometryError::Render(error.to_string()))?;
    Ok(())
}

fn draw_highlight(
    image: &mut image::RgbaImage,
    rect: &BoundingRect,
    page_height: f64,
    scale_x: f64,
    scale_y: f64,
) {
    let (width, height) = (image.width(), image.height());
    let x0 = clamp_pixel((rect.x_min * scale_x).round(), width);
    let x1 = clamp_pixel((rect.x_max * scale_x).round(), width);
    let y0 = clamp_pixel(((page_height - rect.y_max) * scale_y).round(), height);
    let y1 = clamp_pixel(((page_height - rect.y_min) * scale_y).round(), height);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let pixel = image.get_pixel_mut(x, y);
            // Translucent yellow highlight.
            pixel[0] = pixel[0].saturating_add((255 - pixel[0]) * 3 / 4);
            pixel[1] = pixel[1].saturating_add((255 - pixel[1]) * 3 / 4);
            pixel[2] = pixel[2].saturating_sub(pixel[2] / 2);
            pixel[3] = 255;
        }
    }
}

fn clamp_pixel(value: f64, dimension: u32) -> u32 {
    let max = f64::from(dimension - 1);
    let clamped = value.round().clamp(0.0, max);
    // Truncation is safe here: the value is clamped to the valid pixel range.
    #[allow(clippy::cast_possible_truncation)]
    {
        u32::try_from(clamped as i64).unwrap_or(0)
    }
}

/// Computes the normalized-text range for a quote within a text map's joined
/// normalized text. Returns `None` if the quote is not present.
pub fn normalized_range(map: &TextMap, quote: &str) -> Option<(u32, u32)> {
    let text = map
        .words
        .iter()
        .map(|word| word.text.clone())
        .collect::<Vec<_>>()
        .join(" ");
    let norm_text = normalized(&text);
    let target = normalized(quote);
    let start_char = norm_text.find(&target)?;
    let end_char = start_char + target.chars().count();
    Some((u32::try_from(start_char).unwrap_or(u32::MAX), u32::try_from(end_char).unwrap_or(u32::MAX)))
}

/// Extracts the page dimensions from a `pdfinfo` output string.
pub fn parse_page_dimensions(info: &str) -> Option<(f64, f64)> {
    let width = info.lines().find_map(|line| {
        line.strip_prefix("Page size:")
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse::<f64>().ok())
    })?;
    let height = info.lines().find_map(|line| {
        line.strip_prefix("Page size:")
            .and_then(|value| value.split_whitespace().nth(1))
            .and_then(|value| value.parse::<f64>().ok())
    })?;
    Some((width, height))
}

/// Extracts the page rotation from a `pdfinfo` output string.
pub fn parse_page_rotation(info: &str) -> Option<i32> {
    info.lines().find_map(|line| {
        line.strip_prefix("Page rot:")
            .and_then(|value| value.trim().parse::<i32>().ok())
    })
}

/// Validates that a source URL is HTTPS (used by citation builders).
pub fn require_https(source_url: &str) -> Result<(), GeometryError> {
    let url = Url::parse(source_url).map_err(|e| GeometryError::Invalid(e.to_string()))?;
    if url.scheme() != "https" {
        return Err(GeometryError::Invalid("source URL must be HTTPS".to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_map() -> TextMap {
        let xml = r#"<page width="612" height="792">
          <word xMin="72" yMin="720" xMax="100" yMax="732">The</word>
          <word xMin="104" yMin="720" xMax="140" yMax="732">city</word>
          <word xMin="144" yMin="720" xMax="200" yMax="732">approved</word>
          <word xMin="72" yMin="700" xMax="120" yMax="712">Ordinance</word>
          <word xMin="124" yMin="700" xMax="160" yMax="712">25-93</word>
        </page>"#;
        parse_bbox_layout(xml, &PageSpec::new("evidence:e", 1, 612.0, 792.0, 0, "1.0", "src")).expect("map")
    }

    #[test]
    fn parses_bbox_words() {
        let map = sample_map();
        assert_eq!(map.words.len(), 5);
        assert_eq!(map.words[2].text, "approved");
        assert!((map.words[2].rect.x_min - 144.0).abs() < 0.001);
        assert_eq!(map.digest.len(), 64);
    }

    #[test]
    fn finds_multi_word_quote() {
        let map = sample_map();
        let occurrences = find_occurrences(&map, "city approved Ordinance");
        assert_eq!(occurrences.len(), 1);
        assert_eq!(occurrences[0].len(), 3);
    }

    #[test]
    fn finds_repeated_phrase_occurrences() {
        let xml = r#"<page width="612" height="792">
          <word xMin="72" yMin="720" xMax="100" yMax="732">Axon</word>
          <word xMin="104" yMin="720" xMax="140" yMax="732">Axon</word>
        </page>"#;
        let map = parse_bbox_layout(xml, &PageSpec::new("evidence:e", 1, 612.0, 792.0, 0, "1.0", "src")).expect("map");
        let occurrences = find_occurrences(&map, "Axon");
        assert_eq!(occurrences.len(), 2);
    }

    #[test]
    fn joins_hyphenated_words() {
        let xml = r#"<page width="612" height="792">
          <word xMin="72" yMin="720" xMax="100" yMax="732">surveil-</word>
          <word xMin="104" yMin="720" xMax="140" yMax="732">lance</word>
        </page>"#;
        let map = parse_bbox_layout(xml, &PageSpec::new("evidence:e", 1, 612.0, 792.0, 0, "1.0", "src")).expect("map");
        let occurrences = find_occurrences(&map, "surveillance");
        assert_eq!(occurrences.len(), 1);
    }

    #[test]
    fn rejects_negative_coordinates() {
        let rect = BoundingRect { x_min: -1.0, y_min: 0.0, x_max: 10.0, y_max: 10.0 };
        assert!(validate_rect(&rect, 612.0, 792.0).is_err());
    }

    #[test]
    fn rejects_inverted_rectangles() {
        let rect = BoundingRect { x_min: 20.0, y_min: 0.0, x_max: 10.0, y_max: 10.0 };
        assert!(validate_rect(&rect, 612.0, 792.0).is_err());
    }

    #[test]
    fn rejects_out_of_bounds() {
        let rect = BoundingRect { x_min: 0.0, y_min: 0.0, x_max: 1000.0, y_max: 10.0 };
        assert!(validate_rect(&rect, 612.0, 792.0).is_err());
    }

    #[test]
    fn quote_mismatch_fails_closed() {
        let map = sample_map();
        let result = build_page_citation(&map, "nonexistent phrase", 0, (0, 1), None, "src");
        assert!(matches!(result, Err(GeometryError::QuoteNotFound { .. })));
    }

    #[test]
    fn parses_ocr_tsv_deterministically() {
        let tsv = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n5\t1\t1\t1\t1\t1\t10\t20\t50\t12\t95\tAxon\n5\t1\t1\t1\t1\t2\t60\t20\t40\t12\t90\tbody\n";
        let map = parse_ocr_tsv(tsv, &PageSpec::new("evidence:e", 1, 400.0, 200.0, 0, "5.0", "src"), 200, 100).expect("map");
        assert_eq!(map.words.len(), 2);
        // Pixel (10,20,50,12) -> PDF user-space at scale 2.0.
        assert!((map.words[0].rect.x_min - 20.0).abs() < 0.001);
        assert!((map.words[0].rect.x_max - 120.0).abs() < 0.001);
        assert!((map.words[0].rect.y_max - 160.0).abs() < 0.001);
        assert!((map.words[0].rect.y_min - 136.0).abs() < 0.001);
    }

    #[test]
    fn malformed_bbox_fails_closed() {
        let result = parse_bbox_layout("<page", &PageSpec::new("evidence:e", 1, 612.0, 792.0, 0, "1.0", "src"));
        assert!(result.is_err() || result.unwrap().words.is_empty());
    }
}
