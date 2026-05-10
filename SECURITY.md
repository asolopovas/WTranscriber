# Security

## Supported versions

Only the latest minor release is patched.

## Reporting

Email **andrius.solopovas@gmail.com**. Do not open public issues for security reports.

Include:

- Affected version (`wt --version`, or the GUI About dialog)
- Steps to reproduce
- Impact assessment
- Proof-of-concept or logs (redact secrets)

We acknowledge within 72 hours. For high-severity issues a fix or mitigation
is committed within 30 days. Once a patched release is out we publish an
advisory through GitHub Security Advisories.

## Release integrity

- Stable tags `vX.Y.Z` are immutable.
- Every release ships `SHA256SUMS[-<ver>]`. Verify before installing.
- Stable APKs are signed with v2/v3 signatures.
- Full process: `docs/release.md`.
