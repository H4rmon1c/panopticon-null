#!/usr/bin/env python3
"""Exercise the real network-isolated Word extraction path outside a Nix build sandbox."""
import importlib.util
import io
from pathlib import Path
import tempfile
import zipfile

root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('colorado_research', root / 'scripts/colorado-research.py')
research = importlib.util.module_from_spec(spec)
spec.loader.exec_module(research)

with tempfile.TemporaryDirectory() as temporary:
    state = Path(temporary)
    db = research.root_store(state)
    try:
        index = 'https://example.org/agenda'
        url = 'https://example.org/AgendaCenter/ViewFile/Minutes/_09152026-1206'
        page = b'<a href="/AgendaCenter/ViewFile/Minutes/_09152026-1206">Minutes</a>'
        document = io.BytesIO()
        with zipfile.ZipFile(document, 'w', zipfile.ZIP_DEFLATED) as archive:
            archive.writestr('[Content_Types].xml', '<Types/>')
            archive.writestr('word/document.xml',
                '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
                '<w:body><w:p><w:r><w:t>Flock Safety discussion</w:t></w:r></w:p></w:body></w:document>')
        def receipt(source, content):
            return {'url': source, 'retrieved_at': '2026-11-11T00:00:00+00:00', 'status': 200,
                    'resolved_ips': ['8.8.8.8'], 'connected_ip': '8.8.8.8', 'content_type': 'application/octet-stream',
                    'sha256': research.sha(content), 'bytes': len(content)}
        body = document.getvalue()
        source_receipt = receipt(url, body)
        research.archive(state, [(receipt(index, page), page), (source_receipt, body)])
        candidate = {'url': url, 'index_url': index, 'index_sha256': research.sha(page), 'anchor_index': 0}
        result = research.extract_saved_docx(state, 'us-co-county-08051', candidate, source_receipt)
        assert result['state'] == 'matched'
        assert result['finding']['citations'][0]['quote'] == 'Flock Safety discussion'
        assert result['document_format'] == 'docx'
        assert result['docx_deleted_text_excluded'] is True
        print('Real network-isolated Word document extraction passed.')
    finally:
        db.close()
