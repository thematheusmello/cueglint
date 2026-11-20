"""Score the recognized original speech from a live meeting against its fixed transcript."""

import json
from pathlib import Path
import sys


def words(text):
    return "".join(c for c in text.lower() if c.isalnum() or c.isspace()).split()


def edit_distance(reference, observed):
    last = list(range(len(observed) + 1))
    for i, word in enumerate(reference, 1):
        row = [i]
        for j, token in enumerate(observed, 1):
            row.append(min(row[-1] + 1, last[j] + 1, last[j - 1] + (word != token)))
        last = row
    return last[-1]


def score(fixture, meeting, source="system_audio"):
    fixture_key = {"system_audio": "fixture", "microphone": "microphone_fixture"}[source]
    if fixture["id"] != meeting[fixture_key]:
        raise ValueError("unexpected speech fixture")
    reference = words(fixture["ground_truth"])
    observed = words(" ".join(row["text"] for row in meeting["transcript"] if row["source"] == source))
    errors = edit_distance(reference, observed)
    threshold = fixture["acceptance_declared_before_run"]["normalized_word_error_rate_max"]
    wer = errors / len(reference)
    return {"source_commit": meeting["source_commit"], "fixture": fixture["id"],
            "fixture_sha256": fixture["sha256"], "meeting_id": meeting["meeting_id"], "source": source,
            "word_errors": errors, "reference_words": len(reference), "word_error_rate": wer,
            "maximum": threshold, "passed": wer <= threshold}


def file_comparison(fixture, meeting, direct):
    reference = words(fixture["ground_truth"])
    segments = [s for s in meeting["transcript"] if s["source"] == "system_audio"]
    captured = " ".join(s["text"] for s in segments)
    weights, = [f for f in direct["model"]["files"] if f["name"] == "model.safetensors"]
    same_model = bool(segments) and all(
        s["audio"]["model_profile"] == direct["model"]["id"]
        and s["audio"]["model_sha256"] == weights["sha256"]
        and s["audio"]["model_revision"] == weights["revision"]
        and all(s["audio"][key] == direct[key] for key in
                ("backend", "language", "frontend_revision", "frontend_config_sha256"))
        for s in segments)
    return {"fixture": fixture["id"], "fixture_sha256": fixture["sha256"],
            "source_commit": meeting["source_commit"], "meeting_id": meeting["meeting_id"],
            "matching_model_and_frontend": same_model, "reference_words": len(reference),
            "file_text": direct["text"], "captured_text": captured,
            "file_word_errors": edit_distance(reference, words(direct["text"])),
            "captured_word_errors": edit_distance(reference, words(captured)),
            "file_model_elapsed_ms": direct["elapsed_ms"], "file_samples": direct["sample_count"],
            "scope": "Same original system WAV and host. The capture route includes playback, monitor, resampling and endpointing; this comparison does not isolate an individual stage."}


if __name__ == "__main__":
    out = Path(sys.argv[1])
    before_stop = sys.argv[2:] == ["--before-stop"]
    if sys.argv[2:] and not before_stop:
        raise SystemExit("expected an evidence directory and optional --before-stop")
    prefix = "before-stop-" if before_stop else ""
    meeting = json.loads((out / ("before-stop.json" if before_stop else "meeting.json")).read_bytes())
    fixtures = [("fixture.json", "system_audio", "word-errors.json")]
    if meeting["microphone"]:
        fixtures.append(("microphone-fixture.json", "microphone", "microphone-word-errors.json"))
    passed = True
    for metadata, source, report in fixtures:
        result = score(json.loads((out / metadata).read_bytes()), meeting, source)
        result["measurement_phase"] = "before Stop" if before_stop else "saved meeting"
        (out / (prefix + report)).write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(result))
        passed = passed and result["passed"]
    if meeting["microphone"]:
        comparison = file_comparison(json.loads((out / "fixture.json").read_bytes()), meeting,
                                     json.loads((out / "system-file-asr.json").read_bytes()))
        comparison["measurement_phase"] = "before Stop" if before_stop else "saved meeting"
        (out / (prefix + "file-versus-capture.json")).write_text(json.dumps(comparison, indent=2) + "\n")
        print(json.dumps(comparison))
        passed = passed and comparison["matching_model_and_frontend"]
    sys.exit(0 if passed else 1)
