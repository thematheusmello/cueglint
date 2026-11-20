"""Score the recognized original speech from a live meeting against its fixed transcript."""

import json
from pathlib import Path
import sys


def words(text):
    return "".join(c for c in text.lower() if c.isalnum() or c.isspace()).split()


def score(fixture, meeting):
    if fixture["id"] != meeting["fixture"]:
        raise ValueError("unexpected speech fixture")
    reference = words(fixture["ground_truth"])
    observed = words(" ".join(row["text"] for row in meeting["transcript"]))
    last = list(range(len(observed) + 1))
    for i, word in enumerate(reference, 1):
        row = [i]
        for j, token in enumerate(observed, 1):
            row.append(min(row[-1] + 1, last[j] + 1, last[j - 1] + (word != token)))
        last = row
    errors = last[-1]
    threshold = fixture["acceptance_declared_before_run"]["normalized_word_error_rate_max"]
    wer = errors / len(reference)
    return {"source_commit": meeting["source_commit"], "fixture": fixture["id"],
            "fixture_sha256": fixture["sha256"], "meeting_id": meeting["meeting_id"],
            "word_errors": errors, "reference_words": len(reference), "word_error_rate": wer,
            "maximum": threshold, "passed": wer <= threshold}


if __name__ == "__main__":
    out = Path(sys.argv[1])
    result = score(json.loads((out / "fixture.json").read_bytes()),
                   json.loads((out / "meeting.json").read_bytes()))
    (out / "word-errors.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))
    sys.exit(0 if result["passed"] else 1)
