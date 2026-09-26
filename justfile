# just check | just test | just package | just acceptance
# Rust toolchain via rustup (pinned in rust-toolchain.toml).
# Flutter/Dart steps only run once `flutter create --platforms=linux .` has been run in app/ (see README.md).

check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo deny check
    if [ -f app/pubspec.yaml ]; then (cd app && flutter analyze); fi

test:
    cargo test --workspace
    if [ -f app/pubspec.yaml ]; then (cd app && flutter test); fi

package:
    @echo "packaging (.deb) lands in M9 — see 07_product/planning/09_IMPLEMENTATION_PLAN.md T-901..T-908"

acceptance:
    @echo "acceptance scripts A01-A30 land per milestone — see tests/acceptance/ and 07_product/planning/08_TESTING_PLAN.md"
