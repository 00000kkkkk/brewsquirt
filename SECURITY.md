# Security Policy

## Supported versions

| Version | Supported |
| ------- | --------- |
| 0.1.x   | Yes       |
| < 0.1   | No        |

Security fixes are released for the latest minor version only.

## Reporting a vulnerability

Please report vulnerabilities privately through
[GitHub Security Advisories](https://github.com/00000kkkkk/javasquirt/security/advisories/new).
Do not open a public issue, pull request or discussion for a security problem.

Include as much of the following as you can:

- The affected component (`jsq`, `vrotdaem`, the VM, a builtin, the module loader).
- The version or commit hash.
- A minimal `.jsq` program, `squirt.json` or command sequence that reproduces the issue.
- The impact you observed or expect.

## What to expect

- An acknowledgement within 7 days.
- An initial assessment within 14 days.
- A fix, advisory and release as soon as practical, with credit to the reporter unless you prefer
  to stay anonymous.

## Scope

JavaSquirt is not a sandbox. Programs run by `jsq` can read and write files through `squirt:fs`,
read environment variables through `process.env` and exit the process. Running an untrusted
`.jsq` file is equivalent to running an untrusted executable.

Likewise, `vrotdaem gluboko` fetches code from the `file:` and `git:` sources listed in
`squirt.json`. Only install dependencies you trust.

In scope, for example:

- Memory unsafety, panics or crashes in the VM triggered by a `.jsq` program.
- Path traversal when resolving modules or installing packages into `nude_modules/`.
- Command or argument injection through dependency specs in `squirt.json`.
- Vulnerabilities in release artifacts or the build pipeline.
