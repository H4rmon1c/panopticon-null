#!/usr/bin/env python3
"""Extract ordinary DOCX paragraphs; invoked only inside the research sandbox."""
import re
import sys
import xml.etree.ElementTree as ET
from zipfile import ZipFile

MAX_XML = 5 * 1024 * 1024
MAX_OUTPUT = 5 * 1024 * 1024


def extract(path):
    with ZipFile(path) as archive:
        entries = archive.infolist()
        if len(entries) > 1024:
            raise ValueError('too many ZIP entries')
        documents = [item for item in entries if item.filename == 'word/document.xml']
        if len(documents) != 1 or '[Content_Types].xml' not in {item.filename for item in entries}:
            raise ValueError('missing or duplicate DOCX document')
        item = documents[0]
        if (item.flag_bits & 1 or item.file_size > MAX_XML or item.compress_size < 1
                or item.file_size > item.compress_size * 200):
            raise ValueError('DOCX document exceeds extraction bounds')
        with archive.open(item) as member:
            xml = member.read(MAX_XML + 1)
        if len(xml) != item.file_size or len(xml) > MAX_XML or re.search(br'<!\s*(?:DOCTYPE|ENTITY)\b', xml, re.I):
            raise ValueError('invalid or unsafe DOCX XML')
    root = ET.fromstring(xml)
    namespaces = {'http://schemas.openxmlformats.org/wordprocessingml/2006/main',
                  'http://purl.oclc.org/ooxml/wordprocessingml/main'}

    def tag(element):
        if not isinstance(element.tag, str) or not element.tag.startswith('{'):
            return ''
        namespace, _, name = element.tag[1:].partition('}')
        return name if namespace in namespaces else ''

    def text_of(element):
        if tag(element) in ('del', 'moveFrom', 'delText', 'instrText'):
            return ''
        value = element.text or '' if tag(element) == 't' else (' ' if tag(element) in ('tab', 'br') else '')
        return value + ''.join(text_of(child) for child in element)

    def paragraphs(element):
        if tag(element) in ('del', 'moveFrom'):
            return
        if tag(element) == 'p':
            yield text_of(element)
            return
        for child in element:
            yield from paragraphs(child)

    lines = []
    total = 0
    for paragraph in paragraphs(root):
        line = ' '.join(paragraph.split())
        if line:
            encoded = line.encode('utf8')
            total += len(encoded) + 1
            if total > MAX_OUTPUT or len(lines) >= 100000:
                raise ValueError('DOCX text exceeds extraction bounds')
            lines.append(line)
    if not lines:
        raise ValueError('DOCX contains no extractable paragraphs')
    return ('\n'.join(lines) + '\n').encode('utf8')


if __name__ == '__main__':
    try:
        if len(sys.argv) != 2:
            raise ValueError('expected one document')
        sys.stdout.buffer.write(extract(sys.argv[1]))
    except Exception:
        print('DOCX extraction failed or exceeded its bounds', file=sys.stderr)
        raise SystemExit(1)
