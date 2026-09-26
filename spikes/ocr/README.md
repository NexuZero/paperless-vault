# T-108 — OCR spike in the sandbox

Status: DONE. Throwaway spike (Python harness, not the production `pv-sandbox`/`pv-worker`).

## Result

All 5 synthetic fixtures (`fixtures/ocr_pilot/`, 2 English, 2 Arabic, 1 mixed bilingual — see
that directory's `MANIFEST.tsv` for the exact check phrases) were recognised correctly by
Tesseract eng+ara running inside a `bwrap` sandbox:

```
file                                lang   time(s)  peak_rss(MB)  match
01_english_invoice.png              eng       0.168          12.1 PASS
02_english_certificate.png          eng       0.160          12.2 PASS
03_arabic_invoice.png               ara       0.170          12.2 PASS
04_arabic_certificate.png           ara       0.153          12.2 PASS
05_mixed_bilingual_notice.png       mixed     0.233          12.2 PASS

ALL PASS
```

Run with `python3 run_spike.py` from this directory (needs the fixtures generated first:
`python3 ../../fixtures/ocr_pilot/generate.py`).

## What the sandbox actually enforced (`run_spike.py`)

- `bwrap --unshare-all` — no network, no host PID namespace, no host IPC.
- Image delivered via a **real anonymous `memfd_create`** (`os.memfd_create` in Python, passed
  to `bwrap --file FD /input.img`), never written to a named path on the host disk (SR-11).
- Recognised text read back over a **pipe** (`tesseract ... - ... stdout`), never written to a
  host output file either.
- `--ro-bind /usr /usr` (+ symlinks for `/bin`, `/lib`, `/lib64`, `/sbin`) is the only real
  filesystem the sandbox sees, plus a private `--tmpfs /tmp` — no home directory.
- Confirmed empirically: `find /tmp -newer <script>` after the full run showed nothing from
  this process (only unrelated IDE language-server files) — no plaintext leaked to the host.

## Fixture generation notes (`fixtures/ocr_pilot/generate.py`)

- Uses Pillow (`PIL.ImageDraw.text(..., direction=...)`), which needs `libraqm` for correct
  Arabic shaping/joining and RTL layout — confirmed present on this machine
  (`PIL.features.check('raqm')` → `True`). Rendered output was visually spot-checked (not just
  assumed correct) before running OCR against it.
- One bug caught and fixed during generation: an ASCII hyphen between Arabic-Indic digit groups
  rendered as a missing-glyph box (tofu) in Noto Naskh Arabic — replaced with `/` as a date
  separator. Worth remembering for any future Arabic-with-punctuation fixture.
- Tesseract version on this machine is 5.3.4 (apt), not the 5.5.x pin in `02_TRD.md` — noted
  there already as an informational gap for T-108/packaging to resolve, not re-litigated here.
  Recognition worked fine at 5.3.4; the version gap is about the packaged `.deb`'s bundled build,
  not whether OCR itself works.

## Not covered by this spike (acceptance gaps, deferred)

- **PDFium** (PDF rendering into the same pipeline) — this spike only exercises raster images,
  not PDF-to-image extraction. PDFium isn't vendored yet; that's a separate fetch/build task.
- **Worker resource limits "tuned"** — memory here (12 MB peak) is far under the 1 GiB OCR worker
  ceiling in `ENVIRONMENT.md`; no actual rlimit enforcement was applied by this harness (bwrap
  itself doesn't set rlimits — that's `pv-sandbox`'s own job, per the T-107 finding). This spike
  shows OCR *works* and roughly how much it costs, not that limits are enforced yet.
- seccomp/Landlock (same gap as T-107 — deferred to the real Rust `pv-sandbox`/`pv-worker`, T-501).
