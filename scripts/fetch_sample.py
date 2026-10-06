#!/usr/bin/env python3
"""Download RAW source records embedded in Kaikki pages for a small real demo.

This is a maintainer convenience, not the full-dictionary distribution route.
Per-page receipts make the sampled snapshot and its checksums auditable.
"""
import argparse
import concurrent.futures
import hashlib
import html
import json
from pathlib import Path
import re
import urllib.request
from urllib.parse import quote

WORDS = """home house hope horn hospital hotel hot how hold hole horse hour hover
fist palm hand finger clench clenched curl inward bubble air gas liquid soap glass
take set went go good well better houses took taken do done did read reading
word dictionary definition example book page look local fast quick the to of a
with in on from by at for into not as be is are was were can could might
strike closed ball water produce improve run play work use used us hope
""".split() + ["take off", "in spite of", "look up"]


def fetch(word):
    url = f"https://kaikki.org/dictionary/English/meaning/{quote(word[0])}/{quote(word[:2])}/{quote(word)}.html"
    request = urllib.request.Request(url, headers={"User-Agent": "local-english-dict-source-sampler/0.1"})
    body = urllib.request.urlopen(request, timeout=60).read()
    page = body.decode()
    marker = page.find("[Show JSON for raw wiktextract")
    if marker < 0:
        raise ValueError(f"No raw Wiktextract records on {url}")
    block = re.search(r"<pre[^>]*>(.*?)</pre>", page[marker:], re.S)
    if not block:
        raise ValueError(f"No raw JSON block on {url}")
    data = html.unescape(block[1]).strip()
    records = []
    decoder = json.JSONDecoder()
    while data:
        record, end = decoder.raw_decode(data)
        data = data[end:].lstrip()
        if record.get("lang_code") == "en" and record.get("word") == word:
            records.append(record)
    if not records:
        raise ValueError(f"No English records for {word}")
    return records, {"word": word, "url": url, "page_sha256": hashlib.sha256(body).hexdigest()}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", default="data/sample-source")
    args = p.parse_args()
    output = Path(args.output)
    output.mkdir(parents=True, exist_ok=False)
    words = list(dict.fromkeys(WORDS))
    receipts = []
    with (output / "raw.jsonl").open("w") as file, concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for records, receipt in pool.map(fetch, words):
            for record in records:
                file.write(json.dumps(record, ensure_ascii=False) + "\n")
            receipts.append(receipt)
            print(receipt["word"], flush=True)
    (output / "receipts.json").write_text(json.dumps(receipts, indent=2))


if __name__ == "__main__":
    main()
