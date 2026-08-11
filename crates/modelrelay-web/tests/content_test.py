import json, pathlib
manifest = json.loads(pathlib.Path("content/integrations/manifest.json").read_text())
assert "variables" in manifest
print("Content manifest valid")
