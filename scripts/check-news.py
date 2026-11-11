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

def _no_dupes(pairs):
    obj = {}
    for k, v in pairs:
        if k in obj:
            raise ValueError(f"duplicate key {k!r}")
        obj[k] = v
    return obj

def _no_constant(name):
    raise ValueError(f"{name} is not valid JSON")

def load(data):
    """Parse as strictly as serde_json: no duplicate keys, no NaN/Infinity, no
    lone UTF-16 surrogates, and schema_version must be a plain int (not a bool
    or 1.0). Raises ValueError otherwise."""
    doc = json.loads(data.decode("utf-8"), object_pairs_hook=_no_dupes,
                     parse_constant=_no_constant)
    # A lone surrogate from a \udXXX escape cannot be encoded to UTF-8.
    json.dumps(doc, ensure_ascii=False).encode("utf-8")
    if isinstance(doc, dict) and "schema_version" in doc \
            and type(doc["schema_version"]) is not int:
        raise ValueError("schema_version must be a plain integer")
    return doc

def ok(path):
    data = path.read_bytes()
    if len(data) > 64 * 1024:
        return False
    try:
        return validator.is_valid(load(data))
    except ValueError:
        return False

failures = []
if not ok(root / "news.json"):
    try:
        detail = "; ".join(e.message for e in validator.iter_errors(
            load((root / "news.json").read_bytes())))
    except ValueError as e:
        detail = str(e)
    failures.append("news/news.json is invalid: " + detail)
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
