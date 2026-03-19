# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| latest  | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in Oxide, please report it responsibly.

**Email:** [nikhil@polkassembly.io](mailto:nikhil@polkassembly.io)

Please include:

- A description of the vulnerability and its potential impact
- Steps to reproduce the issue
- Any relevant logs, screenshots, or proof-of-concept code
- The version or commit hash you tested against

We will acknowledge receipt within **48 hours** and aim to provide an initial assessment within **5 business days**.

## Scope

The following areas are in scope for security reports:

- **WASM sandbox escapes** — Guest modules bypassing capability-based restrictions
- **Host API abuse** — Exploiting granted capabilities beyond their intended scope
- **Memory safety issues** — Bugs in the `wasmtime` integration, fuel metering, or memory limits
- **Network security** — Issues with `.wasm` binary fetching, URL handling, or origin validation
- **Clipboard / file-picker misuse** — Unintended data exfiltration through host peripherals

## Out of Scope

- Denial-of-service via excessive fuel consumption (this is mitigated by design)
- Bugs in upstream dependencies (report those to the respective projects)
- Issues requiring physical access to the user's machine

## Disclosure Policy

- We follow **coordinated disclosure**. Please do not publicly disclose a vulnerability until we have released a fix or 90 days have passed since acknowledgment, whichever comes first.
- Credit will be given to reporters in release notes unless anonymity is requested.

## Security Design

Oxide's security model is built on several layers:

1. **WebAssembly sandboxing** — Guest modules execute in an isolated WASM runtime with no implicit access to the host.
2. **Capability-based permissions** — Host APIs (network, clipboard, etc.) are only available when explicitly granted.
3. **Fuel metering** — Execution is bounded to prevent runaway computation.
4. **Memory limits** — Guest memory is capped to prevent resource exhaustion.
5. **No filesystem or environment access** — Guest modules cannot read or write the host filesystem or environment variables.
