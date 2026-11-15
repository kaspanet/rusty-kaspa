# Security Policy

## Supported Versions

Please report vulnerabilities affecting the current `stable` or `master` branch.
Older revisions and generated artifacts may not receive security updates.

## Reporting a Vulnerability

Please report suspected security vulnerabilities privately through GitHub:

1. Open the repository's [Security advisories page](https://github.com/kaspanet/rusty-kaspa/security/advisories).
2. Click **Report a vulnerability**.
3. Complete and submit the private vulnerability report.

Do not disclose or discuss a suspected vulnerability in public channels (including
GitHub issues, discussions, pull requests, X, or Discord) before it has been
investigated and a fix has been coordinated.

Please include as much of the following information as possible:

- A description of the vulnerability and its potential impact.
- The affected revision, component, and configuration.
- Steps or a minimal example that reproduces the issue.
- Any suggested mitigation or fix.
- Whether the vulnerability has been disclosed elsewhere.

The maintainers will acknowledge the report as soon as practical, investigate it,
and coordinate remediation and disclosure with the reporter. Please keep the report
and related details private until the maintainers confirm that disclosure is safe.

## Regular Issues, Not Security Advisories

Please use the [public issue tracker](https://github.com/kaspanet/rusty-kaspa/issues)
for the following issues when they have no additional security impact:

- Wallet panics.
- RPC or wallet client requests that cause high memory or CPU consumption on the
  server, including slowdown or stalling caused solely by that resource use.
- Issues related to `rothschild`.
- Issues related to `simpa`.

These are regular project issues, not security advisories on their own.
If the same defect also enables a separate security impact, report that impact
privately through the security advisory process above. Use the public issue tracker
for other ordinary bugs, feature requests, and usage questions.
