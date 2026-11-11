#!/usr/bin/env python3
"""Combine the existing Colorado Springs release authorization with captured sources.

This copies private state into a new directory. It never edits either input,
and the Rust production exporter rechecks every imported binding and receipt.
"""

import argparse
import json
import os
from pathlib import Path
import shutil
import sqlite3
import tempfile


def private_dir(path: Path) -> Path:
    path = path.absolute()
    if path.is_symlink() or not path.is_dir():
        raise ValueError(f"private data directory is unsafe: {path}")
    database = path / "pnull.db"
    if database.is_symlink() or not database.is_file():
        raise ValueError(f"private database is unsafe: {database}")
    return path


def copy_tree(source: Path, destination: Path) -> None:
    if not source.is_dir() or source.is_symlink():
        raise ValueError(f"private content tree is unsafe: {source}")
    for root, dirs, files in os.walk(source, followlinks=False):
        base = Path(root)
        if any((base / name).is_symlink() for name in dirs + files):
            raise ValueError(f"private content tree contains a symlink: {base}")
    shutil.copytree(source, destination)
    for entry in destination.rglob("*"):
        os.chmod(entry, 0o700 if entry.is_dir() else 0o600)


def one_authorization(connection: sqlite3.Connection, table: str) -> tuple:
    rows = connection.execute(f"SELECT * FROM {table}").fetchall()
    if len(rows) != 1:
        raise ValueError(f"expected one existing Colorado Springs {table} row")
    item = json.loads(rows[0][-1])
    if table == "review_decisions":
        if (item.get("state") != "approved" or item.get("reviewer") != "h4rmon1c"
                or not item.get("citation_id", "").startswith("atlas-release:")):
            raise ValueError("Colorado Springs release review is not approved")
    elif item.get("field_categories") != ["atlas_release"]:
        raise ValueError("atlas_release authorization is missing")
    return rows[0]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capture-data-dir", required=True, type=Path)
    parser.add_argument("--approval-data-dir", required=True, type=Path)
    parser.add_argument("--output-data-dir", required=True, type=Path)
    args = parser.parse_args()
    capture = private_dir(args.capture_data_dir)
    approval = private_dir(args.approval_data_dir)
    output = args.output_data_dir.absolute()
    if output.exists() or output.is_symlink():
        raise ValueError("output data directory already exists")
    if output.parent.is_symlink() or not output.parent.is_dir():
        raise ValueError("output parent directory is unsafe")
    with tempfile.TemporaryDirectory(prefix=".four-case-store-", dir=output.parent) as temporary:
        stage = Path(temporary)
        os.chmod(stage, 0o700)
        with sqlite3.connect(f"file:{capture / 'pnull.db'}?mode=ro", uri=True) as original:
            with sqlite3.connect(stage / "pnull.db") as target:
                original.backup(target)
                with sqlite3.connect(f"file:{approval / 'pnull.db'}?mode=ro", uri=True) as prior:
                    for table in ("review_decisions", "publication_allowlists"):
                        row = one_authorization(prior, table)
                        placeholders = ",".join("?" for _ in row)
                        target.execute(f"INSERT INTO {table} VALUES ({placeholders})", row)
                target.commit()
                count = target.execute("SELECT count(*) FROM source_reviews").fetchone()[0]
                if count != 12:
                    raise ValueError(f"expected 12 reviewed Colorado endpoints, found {count}")
        os.chmod(stage / "pnull.db", 0o600)
        for directory in ("evidence", "records"):
            copy_tree(capture / directory, stage / directory)
        os.rename(stage, output)
    print(f"prepared private four-case store: {output}")


if __name__ == "__main__":
    main()
