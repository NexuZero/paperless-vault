# vendor/

`cargo vendor` output plus pinned native sources (libsodium tarball, PDFium binary, tessdata)
with checksums. Committed per [[SR-18]] — offline builds must not depend on network access.

Populated at T-002/T-104 once the native sources are fetched and minisign/checksum-verified.
