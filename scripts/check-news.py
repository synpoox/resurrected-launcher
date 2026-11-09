"""Checks news/news.json against news/news.schema.json and the 64 KB cap, then
checks every file in news/fixtures: valid/ must pass the schema and invalid/ must
fail it. The Rust test news_fixtures (src-tauri/src/catalog/news.rs) runs the
same fixtures through News::parse, so the schema and the parser cannot drift.
Needs jsonschema (the workflows pin 4.23.0)."""
import json, pathlib, sys
import jsonschema

root = pathlib.Path(__file__).resolve().parent.parent / "news"
schema = json.loads((root / "news.schema.json").read_text(encoding="utf-8"))
jsonschema.Draft202012Validator.check_schema(schema)
validator = jsonschema.Draft202012Validator(schema, format_checker=jsonschema.FormatChecker())

def ok(path):
    data = path.read_bytes()
    if len(data) > 64 * 1024:
        return False
    try:
        return validator.is_valid(json.loads(data.decode("utf-8")))
    except ValueError:
        return False

failures = []
if not ok(root / "news.json"):
    failures.append("news/news.json is invalid: " + "; ".join(
        e.message for e in validator.iter_errors(json.loads((root / "news.json").read_text(encoding="utf-8")))))
for kind, want in (("valid", True), ("invalid", False)):
    files = sorted((root / "fixtures" / kind).glob("*.json"))
    if not files:
        failures.append(f"no fixtures in news/fixtures/{kind}")
    for f in files:
        if ok(f) != want:
            failures.append(f"news/fixtures/{kind}/{f.name}: schema {'rejects' if want else 'accepts'} it")
for msg in failures:
    print(msg, file=sys.stderr)
sys.exit(1 if failures else 0)
