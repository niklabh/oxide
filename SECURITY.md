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
- **Permission bypasses** — Accessing camera, microphone, geolocation, or screen capture without a user grant, or circumventing manifest capability declarations
- **Cross-origin data leaks** — One app reading another origin's session or persistent storage, or inheriting its permission grants
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
3. **User permission prompts** — Sensitive capabilities (camera, microphone, geolocation, screen capture) require an explicit in-browser Allow per origin; decisions are remembered per `(origin, capability)`.
4. **Manifest capability declarations** — Apps shipping a manifest must declare the sensitive capabilities they may request; undeclared APIs are denied without prompting.
5. **Origin-scoped storage** — Session and persistent key-value storage are isolated per app origin; session storage is cleared on cross-origin navigation.
6. **Fuel metering** — Execution is bounded to prevent runaway computation.
7. **Memory limits** — Guest memory is capped to prevent resource exhaustion.
8. **No filesystem or environment access** — Guest modules cannot read or write the host filesystem or environment variables.
