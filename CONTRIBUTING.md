# Contributing to Kaspa

Thanks for your interest in contributing to Kaspa!

We welcome contributions of all sizes and there are many opportunities to contribute at any level — from clarifying documentation and fixing small bugs to implementing full features and reviewing pull requests.

Reach out to `@Node Developers` in Discord in the [#development](https://discord.com/channels/599153230659846165/755890250643144788) channel.

Follow along the R&D Telegram group [@kasparnd](https://t.me/kasparnd).

## Quick summary

- Open a GitHub Issue or Pull Request to start any discussion. Use Issues for design or spec discussions and PRs when you have code to share.
- Look for `good first issue` if you're getting started; these are intentionally approachable.
- Write detailed pull requests descriptions: explain what you changed, why, design decisions, and any trade-offs.
- **Anyone** willing to contribute is encouraged to review pull requests and ask questions. Your approval and review counts!

## Reviewing Pull Requests

If you can meaningfully review a pull request, please do so even if you have not contributed code to the repo. This helps in improving the quality of code and gives you a great opportunity to learn more about the codebase through the context of a change.

- Leave review comments, ask clarifying questions, request documentation, point out potential regressions.
- Even if you can't read the code but know how to test it, do that too! Ask for information on how to test the change if it's missing from the PR and run it.
- Use Approve when you believe the change is correct and safe to merge.
- Use Request Changes when you find real issues; explain the issue and prefer actionable guidance.

## How to get started

1. Find an issue (or open one) — good first issues are a great first step.
2. Fork the repo (See [Installation](https://github.com/kaspanet/rusty-kaspa?tab=readme-ov-file#installation) guide) and create a feature branch with a short, descriptive name.
3. Implement your change and include tests where appropriate.
4. Make each commit atomic and focused. Update tests or add new ones in the same commit that changes behaviour.
5. Push to your fork and open a Pull Request against the `master` branch (or the branch named in the issue).

## Pull request guidelines

### Before making a Pull Request:
- Run `./check` (or `./check.ps1` on windows) to make sure your code adheres to coding standards
- Run `./test` (or `cargo nextest run --release` on windows) and make sure you all tests still pass

### Please make your PRs easy to review. A helpful PR contains:

- A clear, descriptive title of what the PR does.
- A summary of what changed and the motivation.
- Any relevant background or links to design discussions or Issues.
- A short description of how the change was tested (unit tests, integration tests, manual steps). Reviewers will use this to test your changes.
- Notes about backwards-compatibility, migrations, or behaviour changes.
- If the change is large, consider splitting it into a small series of focused PRs.

### Commit message tips:

- Start with a short subject line (<= 50 chars), leave a blank line, then add details.
- Try to keep your commits atomic as this makes reviewing them in the context of a PR easier, making the PR overall easier to review and eventually merge

## Using Issues and Pull Requests for discussion

- Use a GitHub Issue to propose or discuss ideas before writing code if the change affects APIs, consensus, or requires design feedback.
- You can also contribute by participating in existing discussions.
- When you start implementing, link the Issue in your Pull Request and mention any related discussions.
- If a PR is experimental or a work in progress, create the Pull Request in your fork of the repository first.

## Testing and CI

Add or update tests for behavior changes. Ensure CI passes before requesting a merge. If your change requires a special test or manual validation, describe it in the PR.

## Code of conduct

Be respectful and constructive in discussions. We expect contributors to follow common open-source etiquette; if you're unsure about tone, err on the side of politeness.

## Thank You

Thanks for helping make Kaspa better. If you have questions, reach out to the channels described at the top of this document

