#!/usr/bin/env bash
set -euo pipefail
umask 0022

info() { printf '[sandbox] %s\n' "$*" >&2; }

verify_policy() {
    local policy=/etc/github-copilot/policy.d/copilot-sandbox.json
    if [[ ! -f $policy || -L $policy || $(stat -c '%a:%u:%g' "$policy") != 444:0:0 ]]; then
        info "Refusing to start: $policy must be a regular file owned by root:root with mode 0444."
        return 1
    fi
}

configure_git() {
    git config --global user.name "${QDK_GIT_NAME:-agent}"
    git config --global user.email "${QDK_GIT_EMAIL:-agent@localhost}"
    git config --global --replace-all safe.directory "$SEED"
    git config --global --add safe.directory "$OUTBOX"
    git config --global remote.pushDefault outbox
}

select_branch() {
    local branch=$1
    git check-ref-format --branch "$branch" >/dev/null
    if [[ -n $(git status --porcelain) ]]; then
        info "Refusing to switch branches: the worktree has tracked or untracked changes."
        return 1
    fi
    if git show-ref --verify --quiet "refs/heads/$branch"; then
        git switch --no-guess "$branch"
    elif git show-ref --verify --quiet "refs/remotes/seed/$branch"; then
        git switch --track -c "$branch" "seed/$branch"
    else
        git switch -c "$branch"
    fi
}

setup_repo() {
    local initial=false reference branch start ahead behind
    [[ -d $SEED && -d $OUTBOX ]] || { info "Seed and outbox mounts are required."; return 1; }
    [[ ${QDK_SEED_SHA:-} =~ ^[0-9a-fA-F]{40,64}$ ]] || {
        info "QDK_SEED_SHA must be the host HEAD object ID."
        return 1
    }
    if [[ ! -e $REPO/.git ]]; then
        [[ ! -d $REPO || -z $(ls -A "$REPO") ]] || {
            info "Refusing to clone over a nonempty directory."
            return 1
        }
        git clone --no-local --no-checkout --no-tags --origin seed "$SEED" "$REPO"
        initial=true
    fi
    cd "$REPO"
    git config remote.seed.url "$SEED"
    git config remote.seed.pushurl DISABLED-fetch-only
    git config --replace-all remote.seed.fetch '+refs/heads/*:refs/remotes/seed/*'
    git config remote.seed.tagOpt --no-tags
    git config remote.origin.url https://github.com/microsoft/qdk.git
    git config remote.origin.pushurl DISABLED-fetch-only
    git config --replace-all remote.origin.fetch '+refs/heads/*:refs/remotes/origin/*'
    git config remote.outbox.url DISABLED-push-only
    git config remote.outbox.pushurl "$OUTBOX"
    git config --unset-all remote.outbox.fetch || true
    git config core.hooksPath "$SANDBOX/runtime/hooks"
    git fetch --quiet --no-tags --prune seed '+refs/heads/*:refs/remotes/seed/*'
    if $initial; then
        if [[ -n ${QDK_SEED_BRANCH:-} ]]; then
            git check-ref-format --branch "$QDK_SEED_BRANCH" >/dev/null
            git show-ref --verify --quiet "refs/remotes/seed/$QDK_SEED_BRANCH"
            branch=$QDK_SEED_BRANCH
            start=$QDK_SEED_SHA
        else
            branch=sandbox/${QDK_SEED_SHA:0:12}
            start=$QDK_SEED_SHA
        fi
        git cat-file -e "$start^{commit}"
        git symbolic-ref HEAD "refs/heads/$branch"
        git update-ref "refs/heads/$branch" "$start"
        git reset --hard --quiet "$start"
    fi
    if [[ -n ${QDK_BRANCH:-} ]]; then
        select_branch "$QDK_BRANCH"
    fi
    local current
    current=$(git branch --show-current)
    for branch in "$current" main; do
        [[ -n $branch ]] || continue
        reference=refs/remotes/seed/$branch
        if git show-ref --verify --quiet "$reference"; then
            read -r ahead behind < <(git rev-list --left-right --count "HEAD...$reference")
            info "Against seed/$branch: ahead $ahead, behind $behind."
        else
            info "Against seed/$branch: unavailable."
        fi
        [[ $current != main ]] || break
    done
}

load_token() {
    local path=${1:-/run/secrets/gh_token}
    [[ -s $path ]] || { info 'The gh_token Compose secret is required.'; return 1; }
    COPILOT_GITHUB_TOKEN=$(<"$path")
    GH_TOKEN=$COPILOT_GITHUB_TOKEN
    export COPILOT_GITHUB_TOKEN GH_TOKEN
}

run_child() {
    local child status=0
    forward_signal() {
        kill -s "$1" "$child" 2>/dev/null || true
    }
    trap 'forward_signal TERM' TERM
    trap 'forward_signal INT' INT
    trap 'forward_signal HUP' HUP
    "$@" <&0 &
    child=$!
    while true; do
        wait "$child" && status=0 || status=$?
        kill -0 "$child" 2>/dev/null || break
    done
    trap - TERM INT HUP
    return "$status"
}

sync_outbox() {
    if [[ -d $REPO/.git ]]; then
        (cd "$REPO" && bash "$SANDBOX/runtime/sandbox-push" --quiet) ||
            info "Outbox sync failed; work remains in /work/qdk."
    fi
}

main() {
    verify_policy
    REPO=/work/qdk
    SEED=/seed.git
    OUTBOX=/outbox.git
    SANDBOX=/opt/sandbox
    export QDK_LIB_OVERRIDE_MICROSOFT_QDK=$REPO
    configure_git
    trap sync_outbox EXIT
    setup_repo
    load_token
    run_child "$@"
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    main "$@"
fi
