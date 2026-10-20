# Contributing to Edgeway

Thanks for helping. Edgeway stays small on purpose, so the best contributions
make it more reliable, more secure, or available on another platform.

## Ground rules

- **Small and native.** No Electron, web runtimes, large UI frameworks,
  databases or new network services. New dependencies in the core need a
  clear reason; the release binary size is checked on every build.
- **Local only.** No accounts, cloud features, telemetry or analytics.
- **Honest.** If something can't be done within operating-system rules, say
  so in `docs/LIMITATIONS.md` instead of working around security.
- **Native design.** On macOS, follow the Human Interface Guidelines: system
  controls, SF Symbols, semantic colours, Dark Mode, VoiceOver labels.

## Getting started

Follow [docs/BUILDING.md](docs/BUILDING.md), then:

```bash
cd core && cargo test
```

```bash
scripts/build-macos.sh
```

`examples/peer` and `--profile`/`--dry-run` let you test two devices on one
machine. `scripts/render-previews.sh` renders every window for design review.

## Code

- Rust: `cargo fmt`, no warnings, unit tests next to the code and end-to-end
  tests in `core/tests/`. No panics on input from the network; never block
  the input path.
- Swift: match the existing style; keep UI on the main thread and never do
  work on the event-tap thread beyond calling the core.
- Comments explain *why*, not *what*.
- Protocol changes update `docs/PROTOCOL.md` in the same change and keep
  older peers working (append fields; ignore unknown messages).

## Pull requests

- One topic per pull request, with a clear description of the change and how
  you tested it (include the two-Mac checklist items from
  [docs/TESTING.md](docs/TESTING.md) when input behaviour changes).
- CI must pass.

## Reporting bugs

Open an issue with your OS versions, Mac models, network (Wi-Fi or Ethernet)
and the steps to reproduce. Logs help: run
`/Applications/Edgeway.app/Contents/MacOS/Edgeway` from Terminal. Security
problems go through private advisories; see
[docs/SECURITY.md](docs/SECURITY.md#reporting-a-vulnerability).

## License

By contributing you agree that your contributions are licensed under the
[MIT License](LICENSE).
