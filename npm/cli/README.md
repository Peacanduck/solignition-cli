# solignition

CLI tool for deploying Solana programs without upfront capital via the [Solignition](https://github.com/ORG/solignition-cli) lending protocol.

```sh
npm install -g solignition
solignition init
```

This package is a thin wrapper that resolves the appropriate prebuilt binary for your platform (`@solignition/cli-<platform>-<arch>`) and executes it. Supported platforms:

- `linux-x64`, `linux-arm64`
- `darwin-x64`, `darwin-arm64`
- `win32-x64`

For full documentation, see the [main README](https://github.com/ORG/solignition-cli).
