#!/usr/bin/env python3
"""Package the mod as a source-only `.hnmod`, the file Horizon's launcher installs and builds.

The archive holds `horizon-native.json` and `mixins/locator_heads/` (its `.rs`
and `.toml` files, the engine crate's included), with fixed dates and modes,
so the same sources always give the same bytes.
`--install` also copies it into the launcher's native mods folder.
"""
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
INSTALLED = Path.home() / '.local/share/horizon/horizon-native-mods'

# The native hook-points version this bundle needs: `gui_layer_pre`
# and `gui_layer_post` (API 139), for hiding the contextual bar layer
# and redrawing it with heads in place of the dots. This is a
# legitimate optional field of the packaged manifest: `Manifest.hooks`
# in crates/horizon-mods/src/index/native.rs, `#[serde(default)]`.
HOOKS = 139


def package():
    compat = json.loads((ROOT / 'compatibility.json').read_text())
    bundle = ROOT / 'mixins' / 'locator_heads'
    mixin = tomllib.loads((bundle / 'horizon-mixin.toml').read_text())
    mod = mixin['mod']
    if mixin.get('compatibility', {}).get('horizon_revision') != compat['horizon_revision']:
        raise SystemExit('horizon-mixin.toml and compatibility.json pin different Horizon revisions')
    if (bundle / 'engine' / 'Cargo.lock').exists():
        raise SystemExit('remove mixins/locator_heads/engine/Cargo.lock: a mod crate builds on Horizon\'s lock')
    manifest = dict(schema=1, id=mod['id'], version=mod['version'], game=compat['game'],
                    horizon=compat['horizon_revision'], mixins=compat['horizon_mixins_revision'],
                    hooks=HOOKS)
    files = {'horizon-native.json': (json.dumps(manifest, indent=2, sort_keys=True) + '\n').encode()}
    for path in sorted(bundle.rglob('*')):
        if path.is_file() and (
            path.suffix in ('.rs', '.toml') or path.name in ('COPYING', 'README.md')
        ):
            files[path.relative_to(ROOT).as_posix()] = path.read_bytes()
    # Release files use the hyphenated public name (like the original's
    # `locator-heads`, and like autologin's `autologin-...`): the
    # internal Starling mod id stays the underscore form in the manifest.
    out = ROOT / 'target' / f"{mod['id'].replace('_', '-')}-{mod['version']}.hnmod"
    out.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(out, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, content in sorted(files.items()):
            info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, content)
    result = dict(file=str(out), files=len(files), size=out.stat().st_size,
                  sha256=hashlib.sha256(out.read_bytes()).hexdigest())
    if '--install' in sys.argv[1:]:
        INSTALLED.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(out, INSTALLED / f"{mod['id']}.hnmod")
        result['installed'] = str(INSTALLED / f"{mod['id']}.hnmod")
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    package()
