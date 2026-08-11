import json, pathlib
# Resolve manifest relative to crate root (one level up from tests/)
crate_root = pathlib.Path(__file__).resolve().parent.parent
manifest_path = crate_root / "content" / "integrations" / "manifest.json"
manifest = json.loads(manifest_path.read_text())
assert "variables" in manifest
print("Content manifest valid")
