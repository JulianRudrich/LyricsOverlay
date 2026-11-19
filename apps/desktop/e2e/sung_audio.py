#!/usr/bin/env python3
"""Synthesizes the "sung" lines for the Sing to find end-to-end check.

Speaks each line with espeak-ng and places it in a slot of SLOT seconds after
LEAD seconds of silence, so line k starts exactly at LEAD + k * SLOT on the
audio clock, the way someone singing along would start each line on time.

    python3 sung_audio.py OUT.wav LEAD SLOT "first line" "second line" ...

Needs espeak-ng (Debian and Ubuntu: sudo apt install espeak-ng).
"""

import subprocess
import sys
import tempfile
import wave
from pathlib import Path


def main() -> None:
    out, lead, slot, lines = Path(sys.argv[1]), float(sys.argv[2]), float(sys.argv[3]), sys.argv[4:]
    rate, frames = None, []
    with tempfile.TemporaryDirectory() as scratch:
        for index, line in enumerate(lines):
            spoken = Path(scratch) / f"line{index}.wav"
            subprocess.run(["espeak-ng", "-v", "en-us", "-s", "150", "-w", str(spoken), line], check=True)
            with wave.open(str(spoken)) as part:
                rate = part.getframerate()
                frames.append(part.readframes(part.getnframes()))
    assert rate is not None, "no lines given"
    width = 2
    silence = lambda seconds: b"\0" * (int(seconds * rate) * width)
    audio = bytearray(silence(lead))
    for index, spoken in enumerate(frames):
        start = int((lead + index * slot) * rate) * width
        audio.extend(b"\0" * max(0, start - len(audio)))
        audio.extend(spoken)
    audio.extend(silence(1.0))
    with wave.open(str(out), "wb") as target:
        target.setnchannels(1)
        target.setsampwidth(width)
        target.setframerate(rate)
        target.writeframes(bytes(audio))
    print(f"{out}: {len(lines)} lines, {len(audio) / width / rate:.1f} s at {rate} Hz")


if __name__ == "__main__":
    main()
