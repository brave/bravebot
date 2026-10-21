#!/usr/bin/env bash
#
# The security scan that comments on our pull requests, run here instead.
#
# Nothing in this repository configures that scan: it arrives as an
# organization-level workflow that calls brave/security-action, so the only
# copy of the runner definitions is in that repository. This clones it, pins
# the same tool versions its action.yml pins, and drives the same
# reviewdog.yml runners against this checkout. The findings are the ones the
# bot would post, minus the posting.
#
#   check-reviewdog.sh            what this branch adds, against its merge base
#                                 with upstream/main, or origin/main where there is
#                                 no upstream remote to compare against
#   check-reviewdog.sh --full     the whole tree, however old the finding is
#
# The two modes are the action's own two modes. On a pull request it runs
# baseline_scan_only, which passes opengrep --baseline-commit and filters
# every runner through the diff; on workflow_dispatch it scans everything
# with -filter-mode=nofilter. Dirty files need a scan without the commit-only
# baseline; reviewdog still filters them through the diff.
#
# No model is involved, so this is deterministic and cheap to re-run.

set -uo pipefail

# Pinned to match brave/security-action: OPENGREP_VERSION in
# src/installOpengrep.js, reviewdog_version in actions/main/action.yml. They
# move rarely; when they do, this drifts silently rather than breaking, so
# check them if a finding here disagrees with one on a PR.
OPENGREP_VERSION="v1.30.0"
REVIEWDOG_VERSION="0.17.5"
# Tracks main on purpose: the organization's required `security` check runs the same ref, so
# the rules reported here are the rules CI reports. The tools above are different: they are
# third-party release assets, so each is compared with a sha256 below before it runs.
SECURITY_ACTION_REF="${SECURITY_ACTION_REF:-main}"

# The runners reviewdog.yml defines and reviewdog.sh actually enables. tfsec
# and brakeman are defined there too but commented out of the RUNNERS line;
# leaving them out keeps this honest about what CI reports.
ALL_RUNNERS="safesvg opengrep sveltegrep npm-audit pip-audit"

CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/bravebot-reviewdog"
REPO_ROOT="$(git rev-parse --show-toplevel)"
# Where the branch is measured from. A checkout that has an `upstream/main` is a fork
# checkout, and there `origin` is the fork, whose `main` is only as current as the last
# time somebody updated it. Scanning against that charges this branch with a finding for
# every upstream commit the fork is behind. Without that ref, `origin` is the repository
# itself and is the base.
if git rev-parse --verify --quiet upstream/main >/dev/null; then
    BASE_REF="upstream/main"
else
    BASE_REF="origin/main"
fi
MODE="diff"
RAW=0
RUNNERS=""

# A ref of our own under refs/remotes/origin/, because the opengrep runner
# hardcodes `--baseline-commit origin/$GITHUB_BASE_REF`. Pointing a throwaway
# origin/ ref at whatever commit we resolved is the only way to feed it a base
# it will not mangle, and it means --base accepts a sha or a tag, not just a
# branch that happens to exist on origin.
BASELINE_REF="refs/remotes/origin/__check_reviewdog_base"

usage() {
    cat <<'EOF'
usage: check-reviewdog.sh [--full] [--base REF] [--runners LIST] [--raw]

  --full           Scan the whole tree instead of this branch's changes
  --base REF       Compare against REF (default: upstream/main in a fork
                   checkout, origin/main otherwise)
  --runners LIST   Space or comma separated subset of:
                     safesvg opengrep sveltegrep npm-audit pip-audit
  --raw            Leave the <br> markup in, as the PR comments carry it
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --full) MODE="full"; shift ;;
        --base) BASE_REF="${2:?--base needs a ref}"; shift 2 ;;
        --runners) RUNNERS="${2:?--runners needs a list}"; shift 2 ;;
        --raw) RAW=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "check-reviewdog: unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

say() { printf '\033[0;34m%s\033[0m\n' "$*" >&2; }
die() { printf '\033[0;31mcheck-reviewdog: %s\033[0m\n' "$*" >&2; exit 1; }

for tool in git curl jq ruby python3; do
    command -v "$tool" >/dev/null || die "$tool is required and is not on PATH"
done

# ── the sha256 each downloaded tool has to match ─────────────────────────────

# A release tag does not fix content: whoever owns the release can replace an asset under it,
# and what is downloaded here runs as whoever runs this script. So nothing is made executable,
# extracted or run until its bytes match a digest written down here. The opengrep digests are
# the ones brave/security-action's installOpengrep.js pins (OPENGREP_BIN_SHA256), re-checked
# against the release assets; the reviewdog ones were taken from the v0.17.5 release and match
# its checksums file, once for each tarball and once for the binary inside it, because the
# binary is what a later run executes from the cache. An asset with no line here is refused.
# Moving a version means moving its lines, and nothing else.
pinned_sha256() {
    case "$1" in
        opengrep_manylinux_x86)     echo 35779bdd72e92129c8df2a77f0c55e8c08356801ea92591ef32108d6b28d564c ;;
        opengrep_manylinux_aarch64) echo a5d5a4a58ba5d46ff51e921663da1c2bba38f4b03987f4aeec87f16c6ad3ecae ;;
        opengrep_osx_arm64)         echo 0f5bc3dec09d995c61331a4017b856ede508f90d95b018d95f1dc6166be89fdd ;;
        reviewdog_tar_Linux_x86_64)  echo 8480e1776bd54dfca3167bff4b117ef439fe6fe5869cdabbe4985652dfbcf417 ;;
        reviewdog_tar_Linux_arm64)   echo 7e51a552484582a794d66a71292ec42f3beac795c4ef8765e1d9e73f073c4b44 ;;
        reviewdog_tar_Darwin_x86_64) echo a8405e653df595b02048610460fb345bed73280ed07e1ca09d51c4903a1c0b11 ;;
        reviewdog_tar_Darwin_arm64)  echo 0d670a7777dc9d296f7f3045197a9e744d8b06094df08b35a73540ed56b00448 ;;
        reviewdog_bin_Linux_x86_64)  echo a286d19863df1386d3449884e6b70dca43260e36e9a8e847aec7da5cac87db1e ;;
        reviewdog_bin_Linux_arm64)   echo c59f48a284ca2ae1aef44f9ebde234bc599fa611e1d21e27e922237f9253afdd ;;
        reviewdog_bin_Darwin_x86_64) echo 558f4a26e73cbafea44d920ea1e764b4a081283a21263f1eda5423b77822fd68 ;;
        reviewdog_bin_Darwin_arm64)  echo f83986e51963817b2804c5cf5fc50b6468d342061d993f26bf281f26fde348e6 ;;
    esac
}

sha256_of() {
    if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1
    else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# matches_pin FILE NAME: FILE exists and is exactly the bytes pinned for NAME.
matches_pin() {
    local want
    want="$(pinned_sha256 "$2")"
    [ -n "$want" ] && [ -f "$1" ] && [ "$(sha256_of "$1")" = "$want" ]
}

# ── the pinned tools, downloaded once ────────────────────────────────────────

case "$(uname -s)" in
    Darwin) os="osx"; rd_os="Darwin" ;;
    Linux)  os="linux"; rd_os="Linux" ;;
    *) die "unsupported platform: $(uname -s)" ;;
esac
case "$(uname -m)" in
    arm64|aarch64) og_arch="arm64"; rd_arch="arm64" ;;
    x86_64|amd64)  og_arch="x86"; rd_arch="x86_64" ;;
    *) die "unsupported architecture: $(uname -m)" ;;
esac
# The release names the mac build osx_arm64 but the linux ones manylinux_*.
[ "$os" = "linux" ] && og_asset="opengrep_manylinux_${og_arch/arm64/aarch64}" \
                    || og_asset="opengrep_osx_${og_arch}"

mkdir -p "$CACHE/bin" || die "cannot create $CACHE"

[ -n "$(pinned_sha256 "$og_asset")" ] \
    || die "no sha256 is pinned for $og_asset, so it is not downloaded"
[ -n "$(pinned_sha256 "reviewdog_bin_${rd_os}_${rd_arch}")" ] \
    || die "no sha256 is pinned for reviewdog on ${rd_os}_${rd_arch}, so it is not downloaded"

# A cached binary is compared before anything runs it, so one that was replaced after it was
# fetched is downloaded again, not executed by a version probe.
if ! matches_pin "$CACHE/bin/opengrep" "$og_asset"; then
    say "downloading opengrep $OPENGREP_VERSION"
    rm -f "$CACHE/bin/opengrep" "$CACHE/bin/opengrep.tmp"
    curl -sfL -o "$CACHE/bin/opengrep.tmp" \
        "https://github.com/opengrep/opengrep/releases/download/$OPENGREP_VERSION/$og_asset" \
        || { rm -f "$CACHE/bin/opengrep.tmp"; die "could not download opengrep $OPENGREP_VERSION ($og_asset)"; }
    matches_pin "$CACHE/bin/opengrep.tmp" "$og_asset" \
        || { rm -f "$CACHE/bin/opengrep.tmp"; die "opengrep $OPENGREP_VERSION ($og_asset) does not match its pinned sha256; not running it"; }
    chmod +x "$CACHE/bin/opengrep.tmp"
    mv "$CACHE/bin/opengrep.tmp" "$CACHE/bin/opengrep"
fi

if ! matches_pin "$CACHE/bin/reviewdog" "reviewdog_bin_${rd_os}_${rd_arch}"; then
    say "downloading reviewdog $REVIEWDOG_VERSION"
    rm -f "$CACHE/bin/reviewdog"
    rd_tmp="$(mktemp -d "$CACHE/reviewdog.XXXXXX")" || die "cannot create a scratch directory in $CACHE"
    curl -sfL -o "$rd_tmp/reviewdog.tar.gz" "https://github.com/reviewdog/reviewdog/releases/download/v$REVIEWDOG_VERSION/reviewdog_${REVIEWDOG_VERSION}_${rd_os}_${rd_arch}.tar.gz" \
        || { rm -rf "$rd_tmp"; die "could not download reviewdog $REVIEWDOG_VERSION"; }
    matches_pin "$rd_tmp/reviewdog.tar.gz" "reviewdog_tar_${rd_os}_${rd_arch}" \
        || { rm -rf "$rd_tmp"; die "reviewdog $REVIEWDOG_VERSION does not match its pinned sha256; not extracting it"; }
    tar xzf "$rd_tmp/reviewdog.tar.gz" -C "$rd_tmp" reviewdog \
        || { rm -rf "$rd_tmp"; die "could not extract reviewdog $REVIEWDOG_VERSION"; }
    matches_pin "$rd_tmp/reviewdog" "reviewdog_bin_${rd_os}_${rd_arch}" \
        || { rm -rf "$rd_tmp"; die "the reviewdog binary does not match its pinned sha256; not running it"; }
    mv "$rd_tmp/reviewdog" "$CACHE/bin/reviewdog"
    rm -rf "$rd_tmp"
fi

# The runner definitions and the rule set. A shallow clone, refreshed on every
# run, because the rules are the point: a stale copy reports findings CI does
# not and misses ones it does. Offline, the existing clone is used as-is.
ACTION="$CACHE/security-action"
if [ ! -d "$ACTION/.git" ]; then
    say "cloning brave/security-action ($SECURITY_ACTION_REF)"
    git clone --depth 1 --branch "$SECURITY_ACTION_REF" \
        https://github.com/brave/security-action.git "$ACTION" \
        || die "could not clone brave/security-action"
else
    git -C "$ACTION" fetch --depth 1 origin "$SECURITY_ACTION_REF" --quiet 2>/dev/null \
        && git -C "$ACTION" checkout --quiet FETCH_HEAD \
        || say "could not refresh brave/security-action; using the cached copy"
fi

SCRIPTPATH="$ACTION/assets"
export SCRIPTPATH
export PATH="$CACHE/bin:$PATH"

# safesvg reads its file list with `xargs -0 -n1 -a FILE`, and BSD xargs has no
# -a. Rather than rewrite the runner, hand it an xargs that understands the one
# flag it is missing and delegates the rest.
if ! xargs --version >/dev/null 2>&1; then
    cat > "$CACHE/bin/xargs" <<'SHIM'
#!/bin/sh
# BSD xargs has no -a FILE; read the file on stdin instead.
file=""; args=""
while [ $# -gt 0 ]; do
    case "$1" in
        -a) file="$2"; shift 2 ;;
        -a*) file="${1#-a}"; shift ;;
        *) args="$args $(printf '%s' "$1" | sed "s/'/'\\\\''/g; s/^/'/; s/\$/'/")"; shift ;;
    esac
done
if [ -n "$file" ]; then
    eval "exec /usr/bin/xargs $args" < "$file"
fi
eval "exec /usr/bin/xargs $args"
SHIM
    chmod +x "$CACHE/bin/xargs"
fi

# ── which runners have anything to look at ───────────────────────────────────

cd "$REPO_ROOT" || die "cannot enter $REPO_ROOT"
if [ "$MODE" != "full" ]; then
    BASE_SHA="$(git rev-parse --verify --quiet "${BASE_REF}^{commit}")" \
        || die "no such ref: $BASE_REF (fetch it, or pass --base)"
    # Use the branch point rather than changes that landed on main afterwards.
    MERGE_BASE="$(git merge-base HEAD "$BASE_SHA")" || die "no merge base with $BASE_REF"
fi

# Every runner but opengrep is a no-op without its file type, and pip-audit is
# worse than a no-op: it imports pip_audit at module scope, so with no python
# manifest to audit it still fails to start and reviewdog reports the crash.
# Selecting on content keeps a clean tree quiet.
# Any of the pathspecs matching a tracked file is enough.
has() { [ -n "$(git -C "$REPO_ROOT" ls-files -- "$@" | head -n 1)" ]; }
if [ -z "$RUNNERS" ]; then
    RUNNERS="opengrep"
    has '*.svg' && RUNNERS="$RUNNERS safesvg"
    # This runner ignores Git exclusions while finding extracted scripts. Do not
    # walk build output and nested worktrees when the diff has no script inputs.
    if [ "$MODE" = "full" ]; then
        has '*.svelte' '*.html' && RUNNERS="$RUNNERS sveltegrep"
    else
        script_files="$(git diff --name-only --diff-filter=d "$MERGE_BASE" -- '*.svelte' '*.html')" \
            || die "cannot list script files to scan"
        [ -z "$script_files" ] || RUNNERS="$RUNNERS sveltegrep"
    fi
    has 'package-lock.json' '*/package-lock.json' && RUNNERS="$RUNNERS npm-audit"
    # security-action's npm-audit runs `npm audit --package-lock-only` on package-lock.json files only
    # and has no pnpm equivalent, so pnpm-lock.yaml is audited by neither this script nor the PR scan.
    if has 'pnpm-lock.yaml' '*/pnpm-lock.yaml'; then
        say "pnpm-lock.yaml is not audited: security-action's npm-audit reads only package-lock.json"
    fi
    has 'pyproject.toml' '*/pyproject.toml' 'requirements*.txt' '*/requirements*.txt' \
        && RUNNERS="$RUNNERS pip-audit"
fi
RUNNERS="$(printf '%s' "$RUNNERS" | tr ',' ' ' | tr -s ' ')"

for runner in $RUNNERS; do
    case " $ALL_RUNNERS " in
        *" $runner "*) ;;
        *) die "unknown runner: $runner (have: $ALL_RUNNERS)" ;;
    esac
done

# pip-audit is a library import, not a subprocess, so it needs its own
# interpreter rather than a tool install. Same shape as the action's `uv sync`
# into a venv whose bin goes on PATH ahead of everything else.
case " $RUNNERS " in
    *" pip-audit "*)
        if ! python3 -c 'import pip_audit' >/dev/null 2>&1; then
            if [ ! -x "$CACHE/venv/bin/python3" ]; then
                say "creating a venv for pip-audit"
                python3 -m venv "$CACHE/venv" >/dev/null 2>&1 \
                    && "$CACHE/venv/bin/pip" install --quiet 'pip-audit~=2.9.0' \
                    || say "could not install pip-audit; that runner will report a failure"
            fi
            [ -x "$CACHE/venv/bin/python3" ] && export PATH="$CACHE/venv/bin:$PATH"
        fi
        ;;
esac

# ── the file list every runner scopes itself with ────────────────────────────

# reviewdog writes a stderr log per runner into the working directory, and the
# working directory has to be the repo root for the scan to find anything.
# Clear them going in and out so a run leaves nothing behind.
cleanup() {
    rm -f "$REPO_ROOT"/reviewdog.*.stderr.log
    git update-ref -d "$BASELINE_REF" 2>/dev/null
}
trap cleanup EXIT
rm -f "$REPO_ROOT"/reviewdog.*.stderr.log

FINDINGS="$(mktemp)"
FAILURES="$(mktemp)"
CONFIG="$(mktemp)"
BRAVEBOT_SCAN_FAILURES="$(mktemp)"
export BRAVEBOT_SCAN_FAILURES
trap 'cleanup; rm -f "$FINDINGS" "$FAILURES" "$CONFIG" "$BRAVEBOT_SCAN_FAILURES"' EXIT

# A failed scanner at the start of a pipeline must fail the runner even when
# its formatter exits successfully. Keep the upstream commands and rules.
ruby -ryaml -rshellwords -e '
    config = YAML.load_file(ARGV[0])
    config.fetch("runner").each do |name, runner|
        runner["cmd"] = <<~SH
            bash -o pipefail -c #{Shellwords.escape(runner.fetch("cmd"))}
            status=$?
            if [ "$status" -ne 0 ]; then
                printf "%s\\n" #{Shellwords.escape(name)} >> "$BRAVEBOT_SCAN_FAILURES"
            fi
            exit "$status"
        SH
    end
    puts YAML.dump(config)
' "$SCRIPTPATH/reviewdog/reviewdog.yml" > "$CONFIG" || die "cannot prepare runner configuration"

scan_failed=0
if [ "$MODE" = "full" ]; then
    unset GITHUB_BASE_REF
    # reviewdog.sh's own full-scan line.
    git ls-files | tr '\n' '\0' > "$SCRIPTPATH/all_changed_files.txt" || die "cannot list files to scan"
    say "scanning the whole tree: $RUNNERS"

    reviewdog \
        -runners="$(printf '%s' "$RUNNERS" | tr ' ' ',')" \
        -conf="$CONFIG" \
        -filter-mode=nofilter \
        -reporter=local > "$FINDINGS" 2>>"$FAILURES" || scan_failed=1
else
    if [ "$MERGE_BASE" = "$(git rev-parse HEAD)" ] && git diff --quiet "$MERGE_BASE"; then
        say "HEAD is at the merge base with $BASE_REF; nothing on this branch to scan"
        say "(scan the whole tree with --full)"
        exit 0
    fi

    git update-ref "$BASELINE_REF" "$MERGE_BASE" || die "cannot set scanner baseline"
    # Opengrep derives baseline targets from commits and misses dirty files.
    # Reviewdog still filters the full scan to this branch and working-tree diff.
    if git diff --quiet HEAD; then
        export GITHUB_BASE_REF="__check_reviewdog_base"
    else
        unset GITHUB_BASE_REF
    fi

    if branch_name="$(git symbolic-ref --quiet --short HEAD)"; then
        scan_subject="the \`$branch_name\` branch"
    else
        scan_subject="detached HEAD ($(git rev-parse --short HEAD))"
    fi
    say "scanning $scan_subject against $BASE_REF ($(git rev-parse --short "$MERGE_BASE")): $RUNNERS"
    git diff --name-only -z --diff-filter=d "$MERGE_BASE" \
        > "$SCRIPTPATH/all_changed_files.txt" || die "cannot list files to scan"

    # One runner at a time, filtered through the diff, as reviewdog.sh does it.
    for runner in $RUNNERS; do
        say "starting $runner"
        reviewdog \
            -reporter=local \
            -runners="$runner" \
            -conf="$CONFIG" \
            -diff="git diff -U0 $MERGE_BASE" \
            >> "$FINDINGS" 2>>"$FAILURES" || scan_failed=1
        say "finished $runner"
    done
fi

# ── what came back ───────────────────────────────────────────────────────────

# A runner that cannot start reports zero findings, which reads exactly like a
# clean run. Say so rather than let it pass silently.
# Outside CI, security-action's sandbox wrapper runs the scanner unsandboxed
# and says so on stderr; that line alone is a completed scan, not a failure.
SANDBOX_NOTICE='^with-sandbox: .*; running unsandboxed \(local mode\)\.$'
for runner in $RUNNERS; do
    log="$REPO_ROOT/reviewdog.$runner.stderr.log"
    [ -s "$log" ] || continue
    rest="$(grep -Ev "$SANDBOX_NOTICE" "$log")"
    [ -n "$rest" ] || continue
    printf '\033[0;31m%s could not run cleanly:\033[0m\n' "$runner" >&2
    printf '%s\n' "$rest" | sed 's/^/  /' >&2
    scan_failed=1
done
if [ -s "$FAILURES" ]; then
    sed 's/^/  /' "$FAILURES" >&2
fi
if grep -q 'failed with zero findings: The command itself failed' "$FAILURES"; then
    scan_failed=1
fi
# Reviewdog may return zero when a runner fails after emitting findings that
# the diff filters out. Record the runner status before that filtering happens.
if [ -s "$BRAVEBOT_SCAN_FAILURES" ]; then
    say "failed runners: $(tr '\n' ' ' < "$BRAVEBOT_SCAN_FAILURES")"
    scan_failed=1
fi
if [ "$scan_failed" -ne 0 ]; then
    cat "$FINDINGS"
    die "security scan failed; findings are incomplete"
fi

if [ ! -s "$FINDINGS" ]; then
    say "no findings"
    exit 0
fi

# The messages are written for a GitHub comment, so they carry <br> and the Cc
# line the bot uses to pull in the security team. Unfold them for a terminal.
if [ "$RAW" -eq 1 ]; then
    cat "$FINDINGS"
else
    sed -E -e 's|(<br ?/?>)+|\n    |g' \
        -e 's|<!-- Category: ([a-z]+) -->|[\1]|g' \
        -e 's|^([^ ]*:[0-9]+:) |\n\1\n    |' "$FINDINGS"
fi

printf '\n\033[0;31msecurity findings reported\033[0m\n' >&2
exit 1
