#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Development-only, file-output fixture generation; never plays or records audio."""
import hashlib
import json
import pathlib
import re
import shutil
import subprocess
import sys
import wave

TEXT = "The cobalt kite is ready. The meeting starts at nine."
SOURCE_REVISION = "4870adfa25b1a32b4361592f1be8a40337c58d6c"
SOURCE_SHA256 = "bb4338102ff3b49a81423da8a1a158b420124b055b60fa76cfb4b18677130a23"
VOICE_SHA256 = "41534c2a22df5dd4f1052ff9e1a33a3ea7bff5a26b5c02bdad5ba8ddb7524704"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pcm_info(path):
    with wave.open(str(path), "rb") as audio:
        assert audio.getnchannels() == 1 and audio.getsampwidth() == 2
        assert audio.getcomptype() == "NONE" and audio.getnframes() > 0
        return {"sample_rate": audio.getframerate(), "frames": audio.getnframes(),
                "pcm_sha256": hashlib.sha256(audio.readframes(audio.getnframes())).hexdigest()}


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: generate-clean-english-v2.py NEW_OUTPUT_DIRECTORY")
    output = pathlib.Path(sys.argv[1])
    output.mkdir(mode=0o700)
    transcript = pathlib.Path(__file__).with_name("clean-english-v2.txt")
    assert transcript.read_text().strip() == TEXT
    synth = pathlib.Path(shutil.which("espeak-ng") or "missing-espeak-ng").resolve(strict=True)
    ffmpeg = pathlib.Path(shutil.which("ffmpeg") or "missing-ffmpeg").resolve(strict=True)
    synth_version = subprocess.check_output([str(synth), "--version"], text=True).strip()
    ffmpeg_version = subprocess.check_output([str(ffmpeg), "-version"], text=True).strip()
    assert "1.52.0" in synth_version and ffmpeg_version.startswith("ffmpeg version 9.0.1 ")
    data = pathlib.Path(re.search(r"Data at: (.+)", synth_version).group(1))
    assert sha(data / "lang/gmw/en-US") == VOICE_SHA256
    native = output / "clean-english-v2.native.wav"
    normalized = output / "clean-english-v2.wav"
    synth_args = [str(synth), "-D", "-v", "en-us", "-s", "145", "-p", "50", "-a", "100", "-g", "0", "-b", "1", "-f", str(transcript), "-w", str(native)]
    normalize_args = [str(ffmpeg), "-nostdin", "-hide_banner", "-loglevel", "error", "-n", "-i", str(native),
                      "-map_metadata", "-1", "-ac", "1", "-af", "aresample=16000:resampler=swr:dither_method=none:exact_rational=1",
                      "-c:a", "pcm_s16le", "-fflags", "+bitexact", "-flags:a", "+bitexact", str(normalized)]
    subprocess.run(synth_args, check=True)
    subprocess.run(normalize_args, check=True)
    native_info, normalized_info = pcm_info(native), pcm_info(normalized)
    assert native_info["sample_rate"] == 22050 and normalized_info["sample_rate"] == 16000
    data_inputs = ["lang/gmw/en-US", "en_dict", "phondata", "phonindex", "phontab", "intonations"]
    portable_synth = ["${ESPEAK_NG}" if value == str(synth) else "clean-english-v2.txt" if value == str(transcript) else "clean-english-v2.native.wav" if value == str(native) else value for value in synth_args]
    portable_normalize = ["${FFMPEG}" if value == str(ffmpeg) else pathlib.Path(value).name if value in [str(native), str(normalized)] else value for value in normalize_args]
    result = {"synthesizer": {"name": "eSpeak NG", "version": "1.52.0", "license": "GPL-3.0-or-later",
              "source_revision": SOURCE_REVISION, "source_archive": "https://github.com/espeak-ng/espeak-ng/archive/refs/tags/1.52.0.tar.gz",
              "source_archive_sha256": SOURCE_SHA256, "binary_sha256": sha(synth), "data_sha256": {name: sha(data / name) for name in data_inputs}, "command": portable_synth},
              "normalizer": {"name": "FFmpeg", "version": "9.0.1", "license": "GPL-3.0-or-later",
              "source_archive": "https://ffmpeg.org/releases/ffmpeg-9.0.1.tar.xz", "source_archive_sha256": "cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635",
              "binary_sha256": sha(ffmpeg), "build_identity": ffmpeg_version, "command": portable_normalize},
              "transcript_sha256": sha(transcript), "recipe_sha256": sha(pathlib.Path(__file__)),
              "native": {"sha256": sha(native), **native_info}, "normalized": {"sha256": sha(normalized), **normalized_info}}
    (output / "clean-english-v2.generation.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
