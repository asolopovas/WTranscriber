# Security

## Supported versions

Only the latest minor is patched.

## Reporting

Email **andrius.solopovas@gmail.com**. Do not open public issues.

Include: affected version, repro steps, impact, PoC (redact secrets).

SLA: ack within 72h, fix or mitigation within 30 days for high severity. Public advisory via GitHub Security Advisories after a patched release.

## Release integrity

- Stable tags `vX.Y.Z` are immutable.
- Every release ships `SHA256SUMS[-<ver>]`. Verify before installing.
- Android APKs on stable are v2/v3 signed.
- Process: `docs/release.md`.
