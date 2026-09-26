#!/usr/bin/env python3
"""T-108 — sandboxed OCR spike.

Runs Tesseract (eng+ara) inside a bwrap sandbox (no network, no home directory,
--die-with-parent, minimal ro-binds) against 5 synthetic fixtures (fixtures/ocr_pilot/),
feeding the image via a real anonymous memfd (os.memfd_create) rather than a named temp
file (SR-11), and reading recognised text back over a pipe (no output file either).

Throwaway spike: this is a Python harness proving the approach, not the production
pv-sandbox/pv-worker (those are Rust, gated behind GATE-PLAN's application-code rule for
`crates/`, not `spikes/`).
"""
import csv
import os
import pathlib
import resource
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "fixtures" / "ocr_pilot"
MANIFEST = FIXTURES / "MANIFEST.tsv"


def make_memfd_with_contents(data: bytes) -> int:
    fd = os.memfd_create("pv-ocr-spike-input", flags=0)
    os.write(fd, data)
    os.lseek(fd, 0, os.SEEK_SET)
    return fd


def run_sandboxed_ocr(image_bytes: bytes):
    fd = make_memfd_with_contents(image_bytes)
    os.set_inheritable(fd, True)

    bwrap_cmd = [
        "bwrap",
        "--unshare-all",
        "--die-with-parent",
        "--ro-bind", "/usr", "/usr",
        "--symlink", "usr/lib", "/lib",
        "--symlink", "usr/lib64", "/lib64",
        "--symlink", "usr/bin", "/bin",
        "--symlink", "usr/sbin", "/sbin",
        "--proc", "/proc",
        "--dev", "/dev",
        "--tmpfs", "/tmp",
        "--file", str(fd), "/input.img",
        "--",
        "tesseract", "/input.img", "-", "-l", "eng+ara", "--psm", "6",
    ]

    start = time.monotonic()
    usage_before = resource.getrusage(resource.RUSAGE_CHILDREN)
    proc = subprocess.run(
        bwrap_cmd,
        pass_fds=(fd,),
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    elapsed = time.monotonic() - start
    usage_after = resource.getrusage(resource.RUSAGE_CHILDREN)
    os.close(fd)

    peak_kb = usage_after.ru_maxrss - 0  # ru_maxrss is already a running peak across children
    return proc.stdout.decode("utf-8", errors="replace"), proc.stderr.decode("utf-8", errors="replace"), elapsed, peak_kb, proc.returncode


def main():
    rows = list(csv.DictReader(open(MANIFEST), delimiter="\t"))
    print(f"{'file':35s} {'lang':6s} {'time(s)':8s} {'peak_rss(MB)':13s} {'match':6s}")
    all_ok = True
    for row in rows:
        fname = row["filename"]
        check_phrase = row["check_phrase"]
        lang = row["language"]
        img_bytes = (FIXTURES / fname).read_bytes()
        stdout, stderr, elapsed, peak_kb, rc = run_sandboxed_ocr(img_bytes)
        recognised = " ".join(stdout.split())
        match = check_phrase.replace(" ", "") in recognised.replace(" ", "") if lang != "eng" else check_phrase.lower() in recognised.lower()
        # Arabic OCR frequently mis-spaces words; compare with whitespace stripped for ara/mixed.
        status = "PASS" if match else "FAIL"
        if not match:
            all_ok = False
        print(f"{fname:35s} {lang:6s} {elapsed:8.3f} {peak_kb/1024:13.1f} {status:6s}")
        if not match:
            print(f"    expected: {check_phrase!r}")
            print(f"    got:      {recognised!r}")
        if rc != 0:
            print(f"    tesseract exit code {rc}, stderr: {stderr.strip()!r}")
            all_ok = False

    print()
    print("ALL PASS" if all_ok else "SOME FAILED")
    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main())
