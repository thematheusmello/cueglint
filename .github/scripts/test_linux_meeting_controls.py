"""Regression controls for the opt-in hosted audio fixture's traffic and transcript receipts."""

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


traffic = load("linux-meeting-traffic")
scores = load("linux-meeting-word-errors")


def sample(rx=0):
    # Linux /proc/net/dev has eight receive columns, then eight transmit columns.
    text = "Inter-|Receive|Transmit\n face|bytes packets|bytes packets\nlo: " + " ".join(
        str(v) for v in [rx, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
    return {"netns": "net:[2]", "interfaces": {"lo": 1}, "counters": traffic.device_counters(text)}


class Controls(unittest.TestCase):
    def test_packet_deltas_and_failed_workers_are_retained(self):
        for received, status, expected in [(0, 0, 0), (90, 0, 1), (0, 3, 1)]:
            with self.subTest(received=received, status=status), tempfile.TemporaryDirectory() as tmp:
                destination = Path(tmp) / "network.json"
                canary = Path(tmp) / "canary.json"
                canary.write_text("{}")
                env = {"CUEGLINT_PARENT_NETNS": "net:[1]", "CUEGLINT_MEETING_NETWORK_REPORT": str(destination),
                       "CUEGLINT_TEST_REVISION": "original-control"}
                args = ["runner", str(canary), sys.executable, "-c", f"raise SystemExit({status})"]
                with patch.dict(traffic.os.environ, env), patch.object(sys, "argv", args), \
                        patch.object(traffic, "snapshot", side_effect=[sample(), sample(received)]):
                    self.assertEqual(traffic.main(), expected)
                result = json.loads(destination.read_text())
                self.assertEqual(result["worker_exit_code"], status)
                self.assertEqual(result["delta"]["rx_bytes"], received)
                self.assertEqual(result["traffic_check_passed"], received == 0)

    def test_counter_columns_and_decreases(self):
        row = "Inter-|Receive|Transmit\n header\nlo: " + " ".join(str(n) for n in range(16))
        self.assertEqual(traffic.device_counters(row)["lo"],
                         {"rx_bytes": 0, "rx_packets": 1, "tx_bytes": 8, "tx_packets": 9})
        self.assertLess(traffic.compare(sample(7), sample(2), "net:[1]")["rx_bytes"], 0)

    def test_host_or_changed_interface_cannot_pass(self):
        with self.assertRaises(ValueError):
            traffic.compare(sample(), sample(), "net:[2]")
        changed = sample()
        changed["interfaces"] = {"lo": 1, "eth0": 2}
        with self.assertRaises(ValueError):
            traffic.compare(sample(), changed, "net:[1]")

    def test_transcript_scores_reject_both_swapped_sources(self):
        def fixture(name, text):
            return {"id": name, "sha256": name, "ground_truth": text,
                    "acceptance_declared_before_run": {"normalized_word_error_rate_max": 0.1}}
        a, b = fixture("a", "copper tree"), fixture("b", "green sail")
        meeting = {"fixture": "a", "microphone_fixture": "b", "meeting_id": "original", "source_commit": "control",
                   "transcript": [{"source": "system_audio", "text": "Copper tree."},
                                  {"source": "microphone", "text": "Green sail."}]}
        self.assertTrue(scores.score(a, meeting)["passed"])
        self.assertTrue(scores.score(b, meeting, "microphone")["passed"])
        meeting["transcript"][0]["source"], meeting["transcript"][1]["source"] = "microphone", "system_audio"
        self.assertFalse(scores.score(a, meeting)["passed"])
        self.assertFalse(scores.score(b, meeting, "microphone")["passed"])


    def test_file_comparison_requires_matched_weights_and_frontend(self):
        fixture = {"id": "test", "sha256": "original", "ground_truth": "paper sail"}
        common = {"backend": "cpu", "language": "en", "frontend_revision": "v1",
                  "frontend_config_sha256": "config"}
        segment = {"source": "system_audio", "text": "paper sale",
                   "audio": {**common, "model_profile": "profile", "model_sha256": "weights", "model_revision": "rev"}}
        meeting = {"source_commit": "source", "meeting_id": "meeting", "transcript": [segment]}
        model = {"id": "profile", "files": [{"name": "model.safetensors", "revision": "rev", "sha256": "weights"}]}
        direct = {**common, "model": model, "text": "paper sail", "sample_count": 16000, "elapsed_ms": 50}
        result = scores.file_comparison(fixture, meeting, direct)
        self.assertTrue(result["matching_model_and_frontend"])
        self.assertEqual((result["file_word_errors"], result["captured_word_errors"]), (0, 1))
        direct["frontend_config_sha256"] = "different"
        self.assertFalse(scores.file_comparison(fixture, meeting, direct)["matching_model_and_frontend"])


if __name__ == "__main__":
    unittest.main()
