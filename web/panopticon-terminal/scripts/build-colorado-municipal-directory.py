#!/usr/bin/env python3
"""Turn one saved CML membership page into neutral municipal website leads.

The input is supplied by an operator after checking source access. URLs in
this directory are leads only; the script never fetches municipal websites.
"""

import argparse
import hashlib
import json
import re
import unicodedata
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit


ROOT = Path(__file__).resolve().parents[3]
GEOGRAPHY = ROOT / "web/panopticon-terminal/geography/colorado-jurisdictions.json"
SOURCE_URL = "https://www.cml.org/home/networking-events/membership/membership"
HEADING = "Links to Colorado cities and towns"
MAX_INPUT_BYTES = 200_000


class MembershipLinks(HTMLParser):
    def __init__(self):
        super().__init__()
        self.entries = []
        self.entry = None

    def handle_starttag(self, tag, attributes):
        if tag == "li":
            if self.entry is not None:
                raise ValueError("nested municipality list entry")
            self.entry = {"name": "", "website_url": None}
        elif tag == "a" and self.entry is not None:
            href = dict(attributes).get("href")
            if self.entry["website_url"] is not None and self.entry["website_url"] != href:
                raise ValueError("multiple websites in one municipality entry")
            self.entry["website_url"] = href

    def handle_data(self, data):
        if self.entry is not None:
            self.entry["name"] += data

    def handle_endtag(self, tag):
        if tag == "li" and self.entry is not None:
            self.entry["name"] = re.sub(r"\s+", " ", self.entry["name"]).strip().rstrip("*").strip()
            self.entries.append(self.entry)
            self.entry = None


def normalized(name):
    name = name.replace("Mt.", "Mount").replace("Sheriden Lake", "Sheridan Lake")
    if name == "Creede":
        name = "City of Creede"
    if name == "Raymer":
        name = "Raymer (New Raymer)"
    name = "".join(c for c in unicodedata.normalize("NFKD", name) if not unicodedata.combining(c))
    return re.sub(r"[^a-z0-9]", "", name.casefold())


def parse_directory(raw):
    if not raw or len(raw) > MAX_INPUT_BYTES:
        raise ValueError("directory HTML is missing or exceeds its byte bound")
    html = raw.decode("utf-8")
    if html.count(HEADING) != 1:
        raise ValueError("municipal directory heading is missing or ambiguous")
    start = html.index("<table", html.index(HEADING))
    end = html.index("</table>", start) + len("</table>")
    parser = MembershipLinks()
    parser.feed(html[start:end])
    if not parser.entries or parser.entry is not None:
        raise ValueError("municipal directory list is empty or malformed")
    return parser.entries


def build(raw, archived_on):
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", archived_on):
        raise ValueError("archived-on must be an ISO date")
    geography = json.loads(GEOGRAPHY.read_text(encoding="utf-8"))
    municipal = [row for row in geography["jurisdictions"] if row["kind"] == "municipality"]
    by_name = {}
    for row in municipal:
        for name in {row["name"], re.sub(r" (city|town)$", "", row["name"])}:
            key = normalized(name)
            if key in by_name and by_name[key]["id"] != row["id"]:
                raise ValueError(f"ambiguous Census municipal name: {name}")
            by_name[key] = row
    mapped = []
    seen = set()
    for entry in parse_directory(raw):
        name = entry["name"]
        row = by_name.get(normalized(name))
        if row is None or row["id"] in seen:
            raise ValueError(f"unmatched or repeated CML municipality: {name}")
        seen.add(row["id"])
        website = entry["website_url"]
        if website is not None:
            parts = urlsplit(website)
            if (parts.scheme not in ("http", "https") or not parts.hostname
                    or parts.username or parts.password or len(website) > 2_048):
                raise ValueError(f"unsafe directory website lead: {name}")
        mapped.append({
            "jurisdiction_ref": row["id"],
            "listing_name": name,
            "website_url": website,
            "source_locator": f"{HEADING} > {name}",
        })
    linked = sum(item["website_url"] is not None for item in mapped)
    http = sum(item["website_url"] is not None and item["website_url"].startswith("http:")
               for item in mapped)
    return {
        "schema_version": 1,
        "purpose": "Municipal website discovery leads only; no site, source policy, decision, vote, contract, or case has been verified by this directory.",
        "source": {"url": SOURCE_URL, "sha256": hashlib.sha256(raw).hexdigest(), "archived_on": archived_on},
        "counts": {"census_municipalities": len(municipal), "listed": len(mapped),
                   "with_website": linked, "without_website": len(mapped) - linked,
                   "http_leads": http, "https_leads": linked - http,
                   "not_listed": len(municipal) - len(mapped)},
        "municipalities": sorted(mapped, key=lambda item: item["jurisdiction_ref"]),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input-html", required=True, type=Path)
    parser.add_argument("--archived-on", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    output = build(args.input_html.read_bytes(), args.archived_on)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"Mapped {output['counts']['listed']} directory entries to Census places; "
          f"{output['counts']['with_website']} website leads")


if __name__ == "__main__":
    main()
