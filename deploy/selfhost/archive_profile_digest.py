"""The fixed-source digest used by pnull source review and archive profiles."""

import hashlib


def config_digest(source):
    fields = (
        "id", "url", "name", "source_type", "documented_at",
        "official_discovery_url", "minimum_interval_seconds", "maximum_bytes",
        "robots_status", "terms_note",
    )
    joined = "\0".join(str(source[field]) for field in fields)
    return hashlib.sha256(joined.encode()).hexdigest()
