# app/

Flutter application (Flutter 3.47.1 / Dart 3.13.1, pinned in [[02_TRD]]).

Flutter SDK is not installed on this machine yet (needs `sudo snap install flutter --classic` —
see repo root README.md; the builder agent cannot run sudo). Once installed, generate the real
scaffold from this directory:

```
flutter create --platforms=linux .
```

That command produces `pubspec.yaml`, `lib/main.dart`, the `linux/` runner and plugin registrant
files correctly for the pinned Flutter version — do not hand-write them. The empty
`lib/src/rust/` (flutter_rust_bridge generated bindings — never hand-edit), `lib/core/`,
`lib/design/` and `lib/l10n/` directories are pre-created per [[02_TRD]] §4 and will be filled in
by later tasks (T-701 onward), not by `flutter create`.
