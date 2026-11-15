# Security Policy

## Supported Versions

Please report vulnerabilities affecting the current `master` branch.
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

## Community-Funded Security Rewards

As [announced on Discord](https://discord.com/channels/599153230659846165/844142778232864809/1559669026156974082),
there is a community-funded initiative to reward independent
security researchers. The current process is:

1. Collect reports, verify the findings, and assess their actual impact.
2. Classify confirmed vulnerabilities as Critical, High, Medium, or Low.
   Currently, Critical is intended for consensus-critical vulnerabilities.
3. Develop, review, and integrate fixes.
4. Collectively agree on a recommended reward target for each finding based on
   its severity and impact.

After a version containing the fixes is released, the maintainers allow time for
operators to upgrade before publicly disclosing the findings. This typically
means waiting a couple of weeks, but the timing depends on the circumstances.
After disclosure, the maintainers open a community fundraiser with a funding
target equal to the sum of the recommended reward targets.

Each fundraiser remains open for 14 days. When it closes, the funds raised are
distributed among the researchers in proportion to their individual recommended
targets, whether the fundraiser raises less or more than its overall target. For
example, if the recommended targets are 100 KAS and 900 KAS, a fundraiser that
raises 500 KAS pays 50 KAS and 450 KAS respectively; if it raises 1,500 KAS, it
pays 150 KAS and 1,350 KAS. Recommended targets determine each researcher's
share; they are neither guaranteed payouts nor maximum rewards.

This is an initial approach that may change based on feedback from researchers,
contributors, and the community.

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
