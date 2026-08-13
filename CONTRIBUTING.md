# Contributing to X-Aegis

Thank you for your interest in building the future of inflation protection on Stellar! This guide will help you contribute effectively.

## 🛠 Tech Stack

*   **Smart Contracts:** Soroban, Rust
*   **AI Engine:** Time-series FX forecasting (Python/FastAPI)
*   **Data:** Central Bank APIs, Market Feeds

## 📝 Commit Guidelines (Strict)

We follow a strict **Modular Commit** philosophy to ensure history is readable and revertible.

**The Golden Rule:**
> "Commit after every meaningful change, not every line."

*   **Meaningful Change:** Completing a function, finishing a fix, adding a feature block, creating a file, or making a significant modification.
*   **Avoid:** Micro-commits for single-line edits unless they are standalone fixes.
*   **Frequency:** Commit often, but only when you finish a logical piece of work.

### Example Commit Messages
*   `feat(contract): implement yield allocation logic`
*   `fix(ui): correct risk visualization chart`
*   `docs: update fx data source list`

## 📋 Issue Tracking

1.  Pick an open issue from the **Issues** tab on GitHub.
2.  When you start, comment on the issue or mark it as "In Progress".
3.  **When Completed:** your PR must reference the issue with `Closes #<number>` so it auto-closes on merge.

## 🧪 Development Workflow

1.  **Clone**: Clone the repo locally.
2.  **Branch**: Create a feature branch (`feat/my-feature`).
3.  **Develop**: Write code following the Style Guide (`STYLE.md`).
4.  **Test**: Run `cargo test` and `cargo clippy --all-targets --all-features -- -D warnings` locally before pushing.
5.  **Commit**: Follow the commit guidelines above.

## ✅ Contribution Guidelines (Project-Wide)

These rules are required for every contribution and are enforced during review.

1.  **Human review required** — all PRs are reviewed by the maintainer before merge. Automated checks only advise; they never approve or merge.
2.  **No agent/AI branches** — do not submit PRs from `agent/*` branches or purely AI-generated work without your own analysis, testing, and ownership of the change.
3.  **Show the work** — include evidence in your PR: `cargo test` passing, and where applicable a screenshot or short demo of the feature in action.
4.  **Link the issue** — every PR description must use `Closes #<issue>`.
5.  **Quality over quantity** — pick issues that move the product forward. Difficulty alone is not value; the change must be user-visible or fix a real problem.

## Getting Help

Read the **Integration Guides** located in the `docs/` directory for detailed setup instructions.
