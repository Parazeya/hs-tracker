"""Verified Extractor inputs for the Tracker's table generator.

Unknown values remain in the installed raw bundle for diagnostics. They are
not promoted into the lookup tables used for live item identification.
"""
from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import re


def _inside(root: Path, name: str) -> Path:
    if not isinstance(name, str) or not name or re.match(r"^[A-Za-z]:", name):
        raise ValueError(f"Invalid bundle path: {name!r}")
    relative = Path(name)
    path = (root / relative).resolve()
    if relative.is_absolute() or not path.is_relative_to(root) or path == root:
        raise ValueError(f"Bundle path escapes its directory: {name}")
    return path


def load_bundle(directory: Path) -> tuple[dict, dict[str, Path]]:
    root = directory.resolve()
    manifest = json.loads((root / "manifest.json").read_text("utf-8"))
    if manifest.get("schema_version") != 1 or not re.fullmatch(
        r"[0-9a-fA-F]{64}", manifest.get("game", {}).get("fingerprint", "")
    ):
        raise ValueError("Expected an Extractor v1 manifest with a game fingerprint")
    inventory = manifest["game"].get("files")
    if not isinstance(inventory, list) or not inventory:
        raise ValueError("Manifest has no source file inventory")
    normalized, names = [], set()
    for source in inventory:
        if not isinstance(source, dict) or not re.fullmatch(r"[0-9a-fA-F]{64}", str(source.get("sha256", ""))) or type(source.get("size")) is not int or not 0 <= source["size"] <= 9_007_199_254_740_991:
            raise ValueError("Invalid game source fingerprint")
        name = source.get("path")
        _inside(root, name)
        if name in names:
            raise ValueError(f"Duplicate game source path: {name}")
        names.add(name)
        normalized.append({"path": name, "sha256": source["sha256"].lower(), "size": source["size"]})
    if not {"Hero_Siege.exe", "data.win"}.issubset(names):
        raise ValueError("Source inventory must include Hero_Siege.exe and data.win")
    normalized.sort(key=lambda source: source["path"].encode("utf-8"))
    inventory_hash = hashlib.sha256(json.dumps(normalized, ensure_ascii=False, separators=(",", ":")).encode("utf-8")).hexdigest()
    if inventory_hash != manifest["game"]["fingerprint"].lower():
        raise ValueError("Game fingerprint does not match its source inventory")
    datasets = manifest.get("datasets")
    if not isinstance(datasets, dict):
        raise ValueError("Bundle has no dataset inventory")
    files = {}
    for name, dataset in datasets.items():
        if dataset.get("status") not in ("complete", "partial"):
            raise ValueError(f"Dataset {name} has no extraction status")
        path = _inside(root, dataset.get("file"))
        with path.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        if digest != str(dataset.get("sha256", "")).lower():
            raise ValueError(f"Dataset {name} hash does not match its manifest")
        files[name] = path
    for name in ("items", "talents", "translationsItem"):
        if name not in files:
            raise ValueError(f"Bundle has no {name} dataset")
    for name in ("items", "talents"):
        rows = json.loads(files[name].read_text("utf-8"))
        if not isinstance(rows, list) or not rows or len(rows) != datasets[name].get("records"):
            raise ValueError(f"Dataset {name} is empty or has an inconsistent record count")
    csv_parent = files["translationsItem"].parent
    for name, path in files.items():
        if name.startswith("translations") and path.suffix == ".csv" and path.parent != csv_parent:
            raise ValueError("Translation CSV datasets must share their source directory")
    return manifest, files


def current_item_rows(rows: list[dict]) -> list[dict]:
    """Build lookup facts only from fields the current extraction answered.

    fromGame includes both decoded assignments and proven engine defaults;
    detailed provenance remains on the raw row. Legacy `metadata.drop` is never
    consulted in canonical mode, so it cannot revive an old location/chance.
    """
    result = copy.deepcopy(rows)
    for row in result:
        meta = row.get("metadata")
        if not isinstance(meta, dict) or not isinstance(meta.get("fromGame"), list):
            raise ValueError("Canonical item is missing per-field provenance")
        trusted = set(meta["fromGame"])
        for field in ("ID", "rarity", "tier", "type", "name", "lvlreq", "Width", "Height"):
            if field not in trusted:
                meta.pop(field, None)
        for field in ("droprate", "chaseDropRate", "dropPlaces"):
            if field not in trusted:
                row.pop(field, None)
        meta.pop("drop", None)
    return result
