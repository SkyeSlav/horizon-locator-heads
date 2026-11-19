#!/usr/bin/env python3
"""Local replica of the validator Starling runs on a `.hnmod` before it
will list it: `Manifest::check()` + `Bundle::read()` in
crates/horizon-mods/src/index/native.rs at the pinned revision.

Usage: python3 tools/check_bundle.py [path/to/file.hnmod]

Checks, in the validator's own terms:
- manifest keys are exactly the known fields (the struct is
  `#[serde(deny_unknown_fields)]`): schema, id, version, game,
  horizon, mixins, hooks (optional)
- check(): schema == 1, valid_id(id) = ^[a-z0-9_]{1,64}$ (NO hyphen),
  version and game non-empty, horizon/mixins 40 lowercase-hex chars,
  hooks != Some(0)
- every non-manifest entry sits under mixins/<manifest.id>/ and is
  .rs/.toml, a data file, or a crate text file (README/LICENSE/NOTICE/...)
- horizon-mixin.toml [mod] id + version and [compatibility]
  horizon_revision equal the manifest's id/version/horizon

Exits non-zero with the failing check named. Run after packaging,
before submitting to the build server.
"""
import json
import re
import sys
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

KNOWN_FIELDS = {"schema", "id", "version", "game", "horizon", "mixins", "hooks"}
VALID_ID = re.compile(r"^[a-z0-9_]{1,64}$")
REVISION = re.compile(r"^[0-9a-f]{40}$")
TEXT_NAMES = ("README", "LICENSE", "LICENCE", "COPYING", "NOTICE")


def fail(msg):
    raise SystemExit(f"FAIL: {msg}")


def allowed(rest: str) -> bool:
    if rest.endswith(".rs") or rest.endswith(".toml"):
        return True
    parts = rest.split("/")
    if len(parts) >= 2 and (parts[0] == "data" or (len(parts) >= 3 and parts[1] == "data")):
        return True
    name = parts[-1]
    return len(parts) <= 2 and (any(name.startswith(p) for p in TEXT_NAMES) or name == ".gitignore")


def main():
    hnmod = sorted((ROOT / "target").glob("locator-heads-*.hnmod"))
    if len(sys.argv) > 1:
        hnmod = [Path(sys.argv[1])]
    if not hnmod:
        raise SystemExit("no packaged .hnmod in target/: run tools/package.py first")
    path = hnmod[-1]
    print(f"bundle: {path}")
    with zipfile.ZipFile(path) as z:
        names = [n for n in z.namelist() if not n.endswith("/")]
        for name in names:
            if ("\\" in name or name.startswith("/") or ":" in name
                    or any(p in ("", ".", "..") for p in name.split("/"))):
                fail(f"invalid native bundle path: {name}")
        raw = z.read("horizon-native.json")
        manifest = json.loads(raw)
    unknown = sorted(set(manifest) - KNOWN_FIELDS)
    if unknown:
        fail(f"unknown manifest fields (deny_unknown_fields): {unknown}")
    if manifest.get("schema") != 1:
        fail(f"schema != 1: {manifest.get('schema')!r}")
    mid = manifest.get("id", "")
    if not VALID_ID.match(mid):
        fail(f"valid_id({mid!r}) is false: only [a-z0-9_] allowed, no hyphen")
    if not manifest.get("version"):
        fail("version is empty")
    if not manifest.get("game"):
        fail("game is empty")
    for key in ("horizon", "mixins"):
        if not REVISION.match(manifest.get(key, "")):
            fail(f"{key} is not 40 lowercase-hex chars: {manifest.get(key)!r}")
    if manifest.get("hooks") == 0:
        fail("hooks == 0")
    print(f"manifest.check() OK: {json.dumps(manifest, sort_keys=True)}")
    prefix = f"mixins/{mid}/"
    for name in names:
        if name == "horizon-native.json":
            continue
        rest = name[len(prefix):] if name.startswith(prefix) else None
        if rest is None:
            fail(f"unexpected native source: {name} (not under {prefix})")
        if not allowed(rest):
            fail(f"unexpected native source: {name}")
    print(f"entries OK: {len(names)} files, all under {prefix} with allowed suffixes")
    with zipfile.ZipFile(path) as z:
        mixin = tomllib.loads(z.read(prefix + "horizon-mixin.toml").decode())
    mod, compat = mixin.get("mod", {}), mixin.get("compatibility", {})
    if mod.get("id") != mid:
        fail(f"mixin [mod] id {mod.get('id')!r} != manifest id {mid!r}")
    if mod.get("version") != manifest["version"]:
        fail("mixin [mod] version != manifest version")
    if compat.get("horizon_revision") != manifest["horizon"]:
        fail("mixin [compatibility] horizon_revision != manifest horizon")
    print("mixin identity OK: [mod] id/version and [compatibility] horizon_revision match manifest")
    print("ALL CHECKS PASSED")


if __name__ == "__main__":
    main()
