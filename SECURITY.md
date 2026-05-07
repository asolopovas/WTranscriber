# Security policy

## Supported versions

Only the latest minor release line receives security fixes.

| Version | Supported          |
| ------- | ------------------ |
| latest  | :white_check_mark: |
| < latest | :x:               |

## Reporting a vulnerability

Please **do not** open public GitHub issues for security reports.

Email **andrius@asolopovas.com** with:

- Affected version (`wtranscriber --version` or `WTranscriber → About`)
- Reproduction steps and impact assessment
- Any proof-of-concept (PoC) or logs (redact secrets)

You will receive an acknowledgement within **72 hours**. Coordinated
disclosure timeline: a fix or mitigation is committed within **30 days**
for high-severity issues; a public advisory is published via GitHub
Security Advisories once a patched release is available.

## Release integrity

- Stable releases are tagged `vX.Y.Z` and **immutable** once published.
- Every release ships a `SHA256SUMS-<ver>` file. Verify before installing.
- Android APKs published on stable channels are signed with v2/v3
  signatures.
- See `docs/release.md` for the full release process and the roadmap to
  Sigstore / SLSA / CRA compliance.
