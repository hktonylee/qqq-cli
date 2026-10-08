"""Build synthetic fixtures from frozen sources into fresh output directory."""

from __future__ import annotations

import argparse
import base64
import hashlib
import io
import json
from pathlib import Path
import sqlite3
import tarfile

ROOT = Path(__file__).resolve().parent
SQLITE_VERSION = "3.53.1"
TIMESTAMP = "2020-01-02T03:04:05.000Z"
IDS = (3, 8, 13, 21, 34, 55, 89, 144)
SEQUENCES = {"tasks": 233, "images": 31, "messages": 37, "events": 89}
LINK = {
    "identity": {"agent": "codex", "kind": "session", "value": "fixture-linked-session"},
    "server": "fixture-server",
    "pane": {
        "workspace_id": "fixture-workspace", "tab_id": "fixture-tab",
        "pane_id": "fixture-pane", "terminal_id": "fixture-terminal",
        "cwd": "/synthetic/project", "foreground_cwd": "/synthetic/project",
        "agent": "codex", "agent_session": {
            "agent": "codex", "kind": "session", "value": "fixture-linked-session",
        },
    },
}
PIXELS = (
    (7, 3, "pixel.png", "image/png", "png", base64.b64decode(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+k7vkAAAAASUVORK5CYII=")),
    (19, 13, "pixel.gif", "image/gif", "gif", base64.b64decode(
        "R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==")),
)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def json_bytes(value: object) -> bytes:
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode()


def write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def task_rows(version: int) -> list[dict[str, object]]:
    """Canonical migration expectations, projected independently from seed."""
    rows = []
    revisions = {3: 7, 8: 3, 13: 11, 21: 2, 34: 4, 55: 5, 89: 6, 144: 9}
    for task_id in IDS:
        status = "in_progress" if task_id in (8, 55) else (
            "completed" if task_id == 3 or (task_id == 34 and version < 4) else (
                "error" if task_id == 34 else "new"))
        owner = "fixture-owner-linked" if task_id == 8 else (
            "fixture-owner-bare" if task_id == 55 else None)
        description = f"Synthetic task {task_id}\nSecond line: 界 fixture"
        if version >= 6 and task_id in (3, 13):
            image_id, extension = (7, "png") if task_id == 3 else (19, "gif")
            description += f"\n![pixel](.qqq/images/{task_id}/{image_id}.{extension})"
        row = {
            "id": task_id, "description": description, "status": status,
            "claim_key": owner, "created_at": TIMESTAMP, "updated_at": TIMESTAMP,
            "parent_id": (3 if task_id == 13 else 8 if task_id == 21 else None) if version >= 2 else None,
            "harness_name": "codex" if task_id == 8 else (
                "claude" if task_id == 55 and version >= 5 else None),
            "harness_session": "fixture-linked-session" if task_id == 8 else (
                "fixture-bare-session" if task_id == 55 and version >= 5 else owner),
            "orchestrator_name": "herdr" if task_id == 8 else None,
            "orchestrator_session": "fixture-server" if task_id == 8 else None,
            "priority": {13: 37, 21: -21, 89: 99, 144: 5}.get(task_id, 0) if version >= 7 else 0,
            "archived": int(task_id == 89 and version >= 8),
            "content_revision": revisions[task_id] if version >= 10 else 1,
            "tags": ["fixture", f"id-{task_id}", "界"] if version >= 12 else [],
        }
        rows.append(row)
    return rows


def history(version: int) -> tuple[list[dict[str, object]], list[dict[str, object]]]:
    messages = [
        {"id": 4, "task_id": 8, "body": "Synthetic progress\nSecond line", "session": "fixture-owner-linked", "created_at": TIMESTAMP},
        {"id": 19, "task_id": 13, "body": "Queued note\nUnicode 界", "session": None, "created_at": TIMESTAMP},
    ]
    actions = [(2, 8, "fixture-owner-linked", "claim"),
               (9, 3, "fixture-former-owner", "complete"),
               (17, 13, "fixture-old-owner", "release")]
    if version >= 4:
        actions.append((26, 34, "fixture-error-owner", "error"))
    if version >= 8:
        actions.extend([(34, 89, "fixture-archiver", "archive"),
                        (38, 3, "fixture-archiver", "unarchive")])
    if version >= 9:
        actions.append((47, 144, "fixture-reopener", "reopen"))
    events = [{"id": event_id, "task_id": task_id, "session": session,
               "action": action, "created_at": TIMESTAMP}
              for event_id, task_id, session, action in actions]
    return messages, events


def expectations(version: int) -> dict[str, object]:
    tasks = task_rows(version)
    messages, events = history(version)
    dependencies = ([{"task_id": 13, "prerequisite_id": 55},
                     {"task_id": 21, "prerequisite_id": 3},
                     {"task_id": 144, "prerequisite_id": 3}] if version >= 11 else [])
    statuses = {task["id"]: task["status"] for task in tasks}
    blockers = {}
    ready = []
    for task in tasks:
        blocked = []
        parent = task["parent_id"]
        if parent is not None and statuses[parent] != "completed":
            blocked.append(parent)
        blocked.extend(edge["prerequisite_id"] for edge in dependencies
                       if edge["task_id"] == task["id"] and statuses[edge["prerequisite_id"]] != "completed")
        if task["status"] == "new" and not task["archived"]:
            if blocked:
                blockers[str(task["id"])] = sorted(set(blocked))
            else:
                ready.append(task["id"])
    ranks = sorted((task for task in tasks if task["id"] in ready),
                   key=lambda task: (-task["priority"], task["id"]))
    return {
        "canonical": {
            "schema_version": 13, "tasks": tasks, "messages": messages, "events": events,
            "images": [{"id": image_id, "task_id": task_id, "name": name,
                        "media_type": media_type, "bytes": len(data)}
                       for image_id, task_id, name, media_type, _, data in PIXELS],
            "links": [{"task_id": 8, "link_json": LINK}],
            "dependencies": dependencies, "processes": [],
            "sequences": [{"name": name, "seq": seq} for name, seq in sorted(SEQUENCES.items())],
        },
        "queue": {"ready_ids": sorted(ready), "blockers": blockers,
                  "selection_id": ranks[0]["id"] if ranks else None,
                  "owners": {"8": "fixture-owner-linked", "55": "fixture-owner-bare"}},
    }


def insert(conn: sqlite3.Connection, table: str, row: dict[str, object]) -> None:
    columns = ",".join(row)
    marks = ",".join("?" for _ in row)
    conn.execute(f"INSERT INTO {table}({columns}) VALUES ({marks})", tuple(row.values()))


def sql_paths(version: int) -> tuple[str, list[str]]:
    release = "v0.1.1" if version <= 9 else "v0.4.0"
    paths = [f"sources/{release}/schema.sql"]
    paths.extend(f"sources/{release}/migrate_v{target}.sql" for target in range(2, min(version, 11) + 1))
    if version >= 12:
        paths.append("sources/schema12/migrate_v12.sql")
    if version >= 13:
        paths.append("sources/schema13/migrate_v13.sql")
    return release, paths


def seed_database(output: Path, version: int) -> dict[str, object]:
    release, paths = sql_paths(version)
    relative = f"databases/schema{version:02}/qqq.db"
    database = output / relative
    database.parent.mkdir(parents=True)
    conn = sqlite3.connect(database)
    conn.execute("PRAGMA page_size=4096")
    for source in paths:
        conn.executescript((ROOT / source).read_text())
    assert conn.execute("PRAGMA user_version").fetchone()[0] == version
    conn.execute("PRAGMA foreign_keys=ON")
    available = {row[1] for row in conn.execute("PRAGMA table_info(tasks)")}
    canonical = expectations(version)
    for task in canonical["canonical"]["tasks"]:
        row = {key: value for key, value in task.items() if key in available}
        owner_column = "owner_session" if version < 3 else "assignee" if version < 5 else "claim_key"
        row[owner_column] = task["claim_key"]
        if "tags" in row:
            row["tags"] = json.dumps(row["tags"], ensure_ascii=False, separators=(",", ":"))
        insert(conn, "tasks", row)
    for table in ("messages", "events"):
        for row in canonical["canonical"][table]:
            insert(conn, table, row)
    insert(conn, "herdr_links", {"task_id": 8, "link_json": json.dumps(LINK, separators=(",", ":"), sort_keys=True)})
    files = []
    images = []
    for image_id, task_id, name, media_type, extension, data in PIXELS:
        row = {"id": image_id, "task_id": task_id, "name": name, "media_type": media_type}
        row["data" if version < 6 else "bytes"] = data if version < 6 else len(data)
        insert(conn, "images", row)
        path = f"images/{task_id}/{image_id}.{extension}"
        images.append({"id": image_id, "task_id": task_id, "path": path,
                       "media_type": media_type, "bytes": len(data), "sha256": digest(data)})
        if version >= 6:
            source = f"databases/schema{version:02}/{path}"
            write(output / source, data)
            files.append({"path": path, "source": source, "bytes": len(data), "sha256": digest(data)})
    if version >= 10:
        for task in canonical["canonical"]["tasks"]:
            conn.execute("UPDATE tasks SET content_revision=? WHERE id=?", (task["content_revision"], task["id"]))
    for edge in canonical["canonical"]["dependencies"]:
        insert(conn, "task_dependencies", edge)
    for name, seq in SEQUENCES.items():
        conn.execute("UPDATE sqlite_sequence SET seq=? WHERE name=?", (seq, name))
    conn.commit()
    assert conn.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    assert not conn.execute("PRAGMA foreign_key_check").fetchall()
    conn.execute("VACUUM")
    conn.close()
    expected = f"expected/schema{version:02}.json"
    expected_data = json_bytes(canonical)
    write(output / expected, expected_data)
    data = database.read_bytes()
    return {"schema_version": version, "database": relative, "bytes": len(data),
            "sha256": digest(data), "source_release": release if version < 12 else None,
            "base_release": release, "sql_sources": paths, "files": files, "images": images,
            "expected": expected, "expected_sha256": digest(expected_data),
            "source_kind": "pinned-development-extension" if version >= 12 else "released-source-definition"}


def snapshot(output: Path, database: dict[str, object]) -> dict[str, object]:
    version = database["schema_version"]
    manifest = {"version": 1, "database": {"bytes": database["bytes"], "sha256": database["sha256"]},
                "images": [{key: image[key] for key in ("path", "bytes", "sha256")}
                           for image in database["images"]]}
    relative = f"snapshots/format1-schema{version:02}.tar"
    target = output / relative
    target.parent.mkdir(exist_ok=True)
    entries = [("manifest.json", json_bytes(manifest)), ("qqq.db", (output / database["database"]).read_bytes())]
    entries.extend((image["path"], (output / image["source"]).read_bytes()) for image in database["files"])
    with tarfile.open(target, "w", format=tarfile.GNU_FORMAT) as archive:
        for name, data in entries:
            header = tarfile.TarInfo(name)
            header.size = len(data)
            header.mode = 0o600
            header.mtime = header.uid = header.gid = 0
            archive.addfile(header, io.BytesIO(data))
    data = target.read_bytes()
    return {"schema_version": version, "format_version": 1, "path": relative,
            "bytes": len(data), "sha256": digest(data), "source_release": database["source_release"],
            "emitted_by_tagged_release": version in (9, 11),
            "format_source": f"sources/{database['base_release']}/snapshot_format.rs",
            "construction": "synthetic-source-derived-gnu-tar"}


def generate(output: Path) -> None:
    if sqlite3.sqlite_version != SQLITE_VERSION:
        raise SystemExit(f"Reproduction requires SQLite {SQLITE_VERSION}; found {sqlite3.sqlite_version}")
    output.mkdir(parents=True, exist_ok=False)
    provenance_data = (ROOT / "provenance.json").read_bytes()
    provenance = json.loads(provenance_data)
    for source in provenance["sources"]:
        assert digest((ROOT / source["path"]).read_bytes()) == source["sha256"], source["path"]
    for _, _, name, _, _, data in PIXELS:
        write(output / "assets" / name, data)
    databases = [seed_database(output, version) for version in range(1, 14)]
    snapshots = [snapshot(output, entry) for entry in databases if entry["schema_version"] >= 9]
    manifest = {
        "version": 1, "sqlite_generation_version": SQLITE_VERSION,
        "source_provenance": "provenance.json", "source_provenance_sha256": digest(provenance_data),
        "support": {"current_schema": 13, "normal_open_schemas": list(range(1, 14)),
                    "snapshot_database_schemas": list(range(9, 14)), "snapshot_format_versions": [1],
                    "released_snapshot_schemas": [9, 11],
                    "exclusions": ["version0-initialization-only", "legacy-title-manual-conversion",
                                   "legacy-pending-manual-conversion", "future-schema-unsupported"],
                    "provenance_notes": ["Schema10 accepted intermediate definition; tagged releases emitted9/11.",
                                         "Schemas12/13 have no release tag; released base plus pinned introducing SQL."]},
        "databases": databases, "snapshots": snapshots,
    }
    write(output / "manifest.json", json_bytes(manifest))
    print(f"Generated {len(databases)} DB fixtures and {len(snapshots)} snapshots in {output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="Fresh directory; existing path refused")
    generate(parser.parse_args().output.resolve())
