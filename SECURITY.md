# Security and cartridge safety

Security fixes target the current beta. The first public version is
**0.1.0-beta.1**; there are no maintained stable-release branches yet. Earlier
private development builds are not supported public releases.
Hardware qualifications and supported operations are listed in
[Support](docs/support.md).

## Report privately

Use GitHub's **Security → Report a vulnerability** when private vulnerability
reporting is available. Include the version, OS, affected input or operation,
impact and a minimal reproduction using synthetic data. If private reporting is
unavailable, open an issue requesting a private reporting channel without exploit
details, credentials, ROM images, saved games or personal information. No separate
security email or response-time guarantee is currently published.

Relevant issues include malicious ROM/metadata parsing, arbitrary file access or
execution, unsafe command construction, bypasses of physical board/source checks,
unintended cartridge writes, and loss of existing backups. Ordinary device support
requests and recoverable read failures belong in the normal issue tracker.

## Preserve evidence

If an operation behaves unexpectedly, stop through the application when possible
and retain its report and existing backups. Do not repeatedly erase or reprogram
a cartridge to reproduce a failure. Disconnect USB before changing cartridges.
Remove personal paths, device serial numbers and private content from diagnostic
material before sharing it.

There is no network-control server in the application. Game identification uses
an offline catalog; optional artwork requests use HTTPS. ROM bytes and checksums
are not uploaded for identification. Keep release checksum and dependency notices
with any redistributed package.
