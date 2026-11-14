# Compiler review workflow (draft)

This drafts a manually triggered, artifact-only compiler review service in
`microsoft/qdk`. It uses ordinary GitHub Actions and Copilot CLI's native Linux
sandbox, not Copilot cloud agent, Agentic Workflows, or the interactive Docker
sandbox. No Azure service connection, personal credential, registry login, or
additional repository is required.

**Status:** implementation draft, not a commissioned service. Local tests do not
establish that a hosted runner enforces the sandbox. Complete the commissioning
checks below before enabling routine reviews.

## Inputs and comparison boundaries

Dispatch [compiler-review.yml](../../.github/workflows/compiler-review.yml) with
either:

- `pr`: a `microsoft/qdk` PR number or URL, including the top PR of a stack.
- `reference` and `candidate`: commits, branches, or tags for release comparisons.

Do not supply both forms. GitHub already restricts manual dispatch to repository
writers. The workflow accepts execution only from the repository's default
branch, so dispatch's branch selector cannot select an alternative controller.
The draft must be merged before that entry point can run; local tests work on the
development branch. Protect changes to the workflow, controller, and toolkit.

The resolver freezes inputs before any candidate code runs. For a PR, it follows
open PRs owning successive base branches, then looks for exact advertised PR tips
among commits between the bottom comparison's merge base and the requested head.
This also discovers stacks whose PRs all target `main` when their current tips are
in the requested head's ancestry. Boundaries are the merge base and each PR tip,
not every individual commit. The toolkit's `layers` contains intermediate tips;
the requested PR tip is `candidate`. Reports must attribute findings to PRs and
include intermediate-layer hazards.

Discovery uses repository metadata, not dependency prose, commit-message guesses,
or an operator-supplied list. Ambiguous owners, non-linear tips, changed base
snapshots, missing objects, and excessive scopes fail explicitly. Rebased/squashed
historical tips that GitHub no longer advertises are not reconstructed. A chain
represented only in external stack tooling may need a future metadata adapter;
do not claim those boundaries were discovered.

Commit-pair reviews compare the supplied endpoints directly, not their merge
base. They do not require one release to be an ancestor of the other.

## Jobs and authority

| Job     | Inputs and work                                                                     | GitHub authority                                       |
| ------- | ----------------------------------------------------------------------------------- | ------------------------------------------------------ |
| Resolve | Protected controller, public Git history, PR metadata; emits pinned scope           | Contents and PR reads                                  |
| Prepare | Fresh VM; restore CI cache, download dependencies, prebuild newest revision         | Contents read; no Copilot requests                     |
| Review  | Another fresh VM; consume prepared files, run mandatory CLI sandbox, collect output | Contents read and organization-billed Copilot requests |

There are no issue/comment/PR/content writes, deployment identities, repository
secrets, or OIDC permission. The optional notifier is deliberately not implemented.
Any future notifier must use a separate job/workflow and publish a fixed link
derived from trusted run metadata, not execute or interpolate report contents.

Preparation runs candidate-controlled builds and third-party code. Its VM and
outputs are untrusted; separation prevents its background processes from carrying
into the review VM. Only wheels and Cargo registry/Git dependency files cross the
boundary. No prepared script, virtual environment, native overlay, or root
configuration is used as orchestration. The dependency archive rejects links,
special files, path traversal, and excessive expanded size/file counts. Dependencies
remain executable untrusted inputs; archive validation does not make them safe.

The review controller and toolkit come from the workflow checkout and are copied
to root-owned `/opt` paths. Copilot runs as the dedicated non-sudo `qdk-review`
account with a constructed environment, not the runner account. It receives only
the scoped inference token, stripped from shell/MCP child environments using
`--secret-env-vars`. No authenticated `gh` credential is supplied. Public reads
are possible, but authenticated issue snapshots/code search may be unavailable;
the report must disclose the resulting coverage limitation.

## Dependencies and restore-only caching

The preparation job restores the same five paths used by `ci.yml`, using
`actions/cache/restore` and the existing `Linux-cargo-` prefix. Identical paths are
important because Actions cache versions include the paths. This can consume a
compatible default-branch CI cache even without an exact lockfile hit. It is an
acceleration, not proof of dependency completeness or build provenance.

There is **no cache save action or automatic post-job save hook**. Local restored
files may change during preparation; those changes do not update shared CI caches.
The review job neither restores nor saves an Actions cache.

Preparation builds `cargo build --locked --release -p qdk --lib` at the newest
commit, installing its Python build/runtime requirements and pinned `qirrunner`
into a temporary environment. It downloads binary wheels for transfer and bundles
Cargo dependency sources, not `~/.cargo/bin`, credentials, or `target`. The existing
CI target cache accelerates preparation but is not accepted as a reviewed native
overlay. Review builds are produced independently by the toolkit.

The initial supported layout is the current `source/qdk_package` layout and Rust
1.95 on Linux x64/Python 3.11. Older releases, other Rust toolchains, Python
requirements without wheels, optional clients, or extra native/system dependencies
may need explicit preparation support. This is not a promise to review every
historical release with the latest environment.

Only the newest revision seeds dependencies. Earlier PR tips and the merge base
can require crates or Python packages no longer needed by that revision. Such
failures must remain visible as incomplete coverage; the agent must not reopen
feeds, silently skip a layer, or substitute another revision. Future preparation
could fetch the union of boundary lockfiles without changing the review policy.

## Sandbox and threat model

The worker contains public inputs and a short-lived, inference-capable Actions
token. The intended residual risks are inaccurate/malicious artifacts, resource
abuse, and limited-token misuse. Generated repros remain untrusted executable
code, especially when downloaded to a developer's privileged machine.

The controller installs root-owned `/etc/github-copilot/managed-settings.json`
with sandboxing mandatory, `failIfUnavailable: true`, bypass forbidden, automatic
developer-tool grants disabled, local MCP/LSP sandboxing required, and no Git/gh
credential injection. User settings provide the same explicit filesystem grants.
The worker can modify its workspace, scratch/build data, and output, but not the
runner's home or trusted controller/toolkit. Local repository instructions and
built-in remote MCP servers are disabled; the prompt names the trusted toolkit.

Cargo, pip, and npm are offline during review. Sandboxed tools can reach only
`github.com` and `api.github.com`, with local/private destinations blocked. GitHub
reads support investigation, but **GitHub can also serve dependency files and
attacker-controlled content**. Thus this blocks package registries and normal
package-manager retrieval, not every possible download from GitHub. Removing
GitHub access entirely would require pre-snapshotting investigation inputs.

There is no second firewall. The native sandbox OS-isolates subprocesses; the
CLI itself holds the token outside that OS boundary, and in-process file tools
enforce policy on a best-effort basis. Hostname policy does not constrain a remote
service's server-side network access, so built-in MCPs and `web_search` are disabled.
This is deliberately lighter than the interactive Docker sandbox. It is not a
claim of containment against kernel exploits or compromise of the CLI itself.

## Reports, diagnostics, and failures

The requested output is `report.md`, Q#/Python files under `repros/`, and Markdown
issue drafts under `issue-drafts/`. A three-hour controller timeout leaves room
within the four-hour review job for teardown, collection, and upload. Preparation
has its own 90-minute budget. Normal GitHub capacity applies; this draft does not
implement a global three-run queue or hard monetary budget.

Artifacts retain for 21 days and are public with the repository. A separate
one-day dependency-transfer artifact is an implementation detail visible in the
run; it contains public dependencies, not reports or credentials. No branch,
issue, PR, comment, or external session export is produced.

The controller captures CLI JSONL/stdout and stderr incrementally outside the
worker's accessible home. `--share` exports a Markdown transcript on normal exit;
a forced termination may leave only partial JSONL. It also records pinned scope,
requested sandbox policy, CLI/model identity, exit status, and elapsed time.
Requested policy is **not evidence that the policy was enforced**.

Before collection, the controller terminates the dedicated account's processes,
including detached descendants. It accepts bounded regular UTF-8 files only, not
symlinks or special files, and never executes repros. Collection redacts the
known token and recognizable GitHub token strings before uploading. Failed
collection/redaction blocks upload, rather than publishing raw fallback logs.
Pattern redaction cannot guarantee removal of transformed or encoded secrets:
do not add sensitive inputs just because redaction exists.

The CLI exit code is not a correctness verdict or proof of complete coverage.
Failure/timeout preserves available artifacts; a missing report is replaced by an
explicit incomplete-review notice. Final review claims and session transcripts
are untrusted, even when controller metadata is authentic.

## Commissioning and remaining decisions

1. Confirm organization-billed CLI requests and the selected model are enabled.
   Set repository variable `COMPILER_REVIEW_MODEL` to an approved model identifier.
   CLI 1.0.97 and Actions are pinned; upgrade them through reviewed changes.
2. Verify Bubblewrap, namespaces, `/dev/net/tun`, proxying, and managed host rules
   on the selected hosted runner. Confirm sandbox startup fails closed, shell
   children cannot see the inference token, forbidden hosts/private endpoints
   fail, and trusted files reject writes. Capture effective-policy and denial
   evidence; do not rely on an agent-authored claim or just the requested JSON.
3. Exercise single-PR, base-branch stack, same-base stack, moved/rebased stack,
   and release-pair inputs against real GitHub metadata. Compare inferred scope
   with expected PR boundaries before spending a full review.
4. Run a small complete review, a missing-old-dependency case, an agent failure,
   and cancellation/timeout. Check partial artifacts, no cache writes, public
   log hygiene, and absence of GitHub write permissions.
5. Measure preparation/archive size, hosted disk/memory, and model cost. Decide
   whether three simultaneous runs need an organization-managed capacity limit,
   and whether public transcripts are acceptable. If not, choose private storage
   before expanding usage; do not add Azure credentials to the worker casually.

Local checks:

```sh
python3 -m unittest discover -s tools/compiler-review-workflow/tests
npm exec -- prettier --write --no-config tools/compiler-review-workflow/README.md .github/workflows/compiler-review.yml
```

Relevant references:

- [CLI authentication in Actions](https://docs.github.com/en/copilot/how-tos/copilot-cli/use-copilot-cli-in-actions)
- [Native sandbox limitations](https://docs.github.com/en/copilot/concepts/security-governance-and-network-settings/about-cloud-and-local-sandboxes)
- [Managed sandbox settings](https://docs.github.com/en/copilot/reference/enterprise-administrators/enterprise-managed-settings#sandbox)
- [Restore-only Actions cache](https://github.com/actions/cache/tree/main/restore)
