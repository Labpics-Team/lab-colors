# Lab Colors

Production-grade color system with ProgramWire v1 runtime.

## Quick Start

See [packages/colors/README.md](packages/colors/README.md) for the public API contract.

### Installation

`ash
npm install @labpics/colors
`

### Usage

`javascript
import init, { compileProgramWire } from "@labpics/colors";

await init();
const runtime = compileProgramWire(programBytes, 1);
const snapshot = runtime.update(observed);
`

## Architecture

- **ProgramWire v1**: Deterministic binary format (magic LCPW, version 1)
- **Typed refusals**: Invalid declarations produce typed errors, not silent coercion
- **Cross-language parity**: JS builder emits byte-identical output to Rust reference
- **Fail-closed security**: Family graphs require explicit trust parameters

## Р—Р°РІРёСЃРёРјРѕСЃС‚Рё

labcolors-core РёРјРµРµС‚ РЅРѕР»СЊ СЂР°РЅС‚Р°Р№Рј-Р·Р°РІРёСЃРёРјРѕСЃС‚РµР№. РџСЂРѕРІРµСЂСЏРµРјС‹Р№ РєРѕРЅС‚СЂР°РєС‚:

`ash
cargo tree -p labcolors-core --edges=no-dev
`

## Development

`ash
cargo test --workspace
npm test
`

## CI Status

All workflows green on main. See [Actions](https://github.com/Labpics-Team/lab-colors/actions) for live status.

## License

MIT