#!/usr/bin/env bash
# =============================================================================
# scripts/jules.sh — Jules API key manager + client wrapper
#
# Wraps the Jules API (https://developers.google.com/jules/api) so the API key
# is NEVER hardcoded, echoed, or committed. The key lives only in .env
# (gitignored) and is read at call time.
#
# Usage:
#   scripts/jules.sh check                     Validate the key (key is masked)
#   scripts/jules.sh sources                   List connected GitHub sources
#   scripts/jules.sh sessions [--limit N]      List recent sessions (default 20)
#   scripts/jules.sh session <id>              Get one session (outputs/PRs)
#   scripts/jules.sh activities <id> [--limit N]   List session activities
#   scripts/jules.sh send <id> "<message>"     Message the agent in a session
#   scripts/jules.sh create <repo> "<prompt>" [--branch main] [--pr] [--approval]
#   scripts/jules.sh approve <id>              Approve a pending plan
#   scripts/jules.sh archive <id>              Archive one session (archived=true)
#   scripts/jules.sh archive-all [--limit N]   Archive every session (all pages)
#   scripts/jules.sh curl <METHOD> <path> [--data '<json>']   Raw passthrough
#
# The key is read from $JULES_API_KEY if set, otherwise from ./.env (repo root).
# Security rules enforced here:
#   * The key is never printed (status output masks it).
#   * The key must never be committed — .env is in .gitignore.
#   * Exposed keys are auto-disabled by Google; rotate in
#     https://jules.google.com/settings#api if this one leaks.
# =============================================================================
set -euo pipefail

BASE_URL="https://jules.googleapis.com/v1alpha"
ENV_FILE="$(cd "$(dirname "$0")/.." && pwd)/.env"

# ── Key resolution ──────────────────────────────────────────────────────────
# Normalize: strip surrounding single/double quotes (the devenv dotenv loader
# exports the key as a literal '"..."' string — length 55 — vs the clean 53).
normalize_key() {
    printf '%s' "$1" | tr -d '\n' | tr -d "'" | tr -d '"'
}

resolve_key() {
    if [ -n "${JULES_API_KEY:-}" ]; then
        normalize_key "$JULES_API_KEY"
        return
    fi
    if [ -f "$ENV_FILE" ]; then
        normalize_key "$(sed -n 's/^JULES_API_KEY=//p' "$ENV_FILE")"
        return
    fi
    return 1
}

KEY="$(resolve_key || true)"
if [ -z "$KEY" ]; then
    echo "ERROR: JULES_API_KEY not found." >&2
    echo "  Set it in $ENV_FILE (JULES_API_KEY=...) or export JULES_API_KEY." >&2
    exit 1
fi

# ── Helpers ─────────────────────────────────────────────────────────────────
api() {
    # api <method> <path> [--data '<json>']
    method="$1"
    path="$2"
    shift 2
    data_args=()
    if [ "${1:-}" = "--data" ]; then
        data_args=(-H "Content-Type: application/json" -d "$2")
    fi
    curl -sS -m 60 -X "$method" \
        -H "X-Goog-Api-Key: $KEY" \
        "${data_args[@]}" \
        "$BASE_URL$path"
}

# Formatter: jq when available, raw otherwise. Chosen ONCE so pipelines never
# re-request the API as a "fallback" (which double-fires on SIGPIPE).
if command -v jq >/dev/null 2>&1; then
    fmt() { jq -r "$1"; }
else
    fmt() { cat; }
fi

# Compact error check: prints an API error message and exits non-zero.
die_on_error() {
    if [ -n "${1:-}" ] && printf '%s' "$1" | jq -e '.error' >/dev/null 2>&1; then
        printf 'ERROR: %s\n' "$(printf '%s' "$1" | jq -r '.error.message // .error')" >&2
        exit 1
    fi
}

require_id() {
    if [ $# -lt 1 ] || [ -z "$1" ]; then
        echo "ERROR: missing session id. Usage: scripts/jules.sh $CMD <id> ..." >&2
        exit 1
    fi
}

# ── Subcommands ─────────────────────────────────────────────────────────────
CMD="${1:-help}"
shift || true

case "$CMD" in
    check)
        echo "Validating Jules API key..."
        resp="$(api GET "/sources?pageSize=1")"
        if printf '%s' "$resp" | jq -e '.error' >/dev/null 2>&1; then
            printf '❌ Key rejected: %s\n' "$(printf '%s' "$resp" | jq -r '.error.message // "unknown"')" >&2
            echo "   Check $ENV_FILE or rotate at https://jules.google.com/settings#api" >&2
            exit 1
        fi
        echo "✅ Key valid (masked: ${KEY:0:4}…${KEY: -4})"
        echo "   Run 'scripts/jules.sh sources' to list connected GitHub repos."
        ;;
    sources)
        api GET "/sources?pageSize=100" | fmt '
            .sources[] | "  " + .id + (if .githubRepo.isPrivate then " (private)" else "" end)
        '
        ;;
    sessions)
        limit="20"
        if [ "${1:-}" = "--limit" ]; then limit="$2"; fi
        api GET "/sessions?pageSize=$limit" | fmt '
            .sessions[] |
            "  " + .id + "  " + (.title // "" | .[0:60]) +
            ([.outputs[]? | select(.pullRequest) | " → PR: " + .pullRequest.url] | join(""))
        '
        ;;
    session)
        require_id "$@"
        api GET "/sessions/$1" | jq . || true
        ;;
    activities)
        require_id "$@"
        id="$1"
        shift
        limit="30"
        if [ "${1:-}" = "--limit" ]; then limit="$2"; fi
        api GET "/sessions/$id/activities?pageSize=$limit" | fmt '
            .activities[] |
            ( .createTime[0:19] + "  [" + .originator + "]  " +
              ( if .planGenerated then "planGenerated"
                elif .planApproved then "planApproved"
                elif .progressUpdated then "progressUpdated — " + (.progressUpdated.title // "")
                elif .sessionCompleted then "sessionCompleted"
                elif .message then "message — " + (.message.text // "")
                else "activity" end ) )
        '
        ;;
    send)
        require_id "$@"
        id="$1"
        prompt="${2:-}"
        if [ -z "$prompt" ]; then
            echo "ERROR: missing message. Usage: scripts/jules.sh send <id> \"<message>\"" >&2
            exit 1
        fi
        api POST "/sessions/$id:sendMessage" --data "$(jq -nc --arg p "$prompt" '{prompt: $p}')" | jq . || true
        echo "Message sent. Poll with: scripts/jules.sh activities $id"
        ;;
    create)
        repo="${1:-}"
        prompt="${2:-}"
        if [ -z "$repo" ] || [ -z "$prompt" ]; then
            echo "ERROR: usage: scripts/jules.sh create <repo> \"<prompt>\" [--branch main] [--pr] [--approval]" >&2
            exit 1
        fi
        shift 2
        branch="main"
        automation_mode=""
        approval="false"
        while [ $# -gt 0 ]; do
            case "$1" in
                --branch) branch="$2"; shift 2 ;;
                --pr) automation_mode="AUTO_CREATE_PR" ;;
                --approval) approval="true" ;;
                *) shift ;;
            esac
        done
        body="$(jq -nc \
            --arg prompt "$prompt" \
            --arg source "sources/github/$repo" \
            --arg branch "$branch" \
            --arg mode "$automation_mode" \
            --arg approval "$approval" \
            '{prompt: $prompt,
              sourceContext: {source: $source, githubRepoContext: {startingBranch: $branch}},
              title: ($prompt | .[0:60]),
              automationMode: ($mode | select(. != "")),
              requirePlanApproval: ($approval == "true")}')"
        api POST "/sessions" --data "$body" > /tmp/jules_create_resp.json || {
            echo "ERROR: create failed" >&2
            cat /tmp/jules_create_resp.json >&2
            exit 1
        }
        if jq -e '.error' /tmp/jules_create_resp.json >/dev/null 2>&1; then
            printf 'ERROR: %s\n' "$(jq -r '.error.message // "unknown"' /tmp/jules_create_resp.json)" >&2
            exit 1
        fi
        sid="$(jq -r '.id' /tmp/jules_create_resp.json)"
        echo "Session created: $sid — $(jq -r '.title' /tmp/jules_create_resp.json)"
        echo "  Poll:       scripts/jules.sh activities $sid"
        echo "  Approve if required: scripts/jules.sh approve $sid"
        ;;
    approve)
        require_id "$@"
        api POST "/sessions/$1:approvePlan" | jq . || true
        echo "Plan approved for session $1"
        ;;
    archive)
        require_id "$@"
        resp="$(api POST "/sessions/$1:archive")"
        die_on_error "$resp"
        printf '%s' "$resp" | jq -r '"Archived: " + .id + " (state: " + .state + ", archived: " + ((.archived // false) | tostring) + ")"'
        ;;
    archive-all)
        # Archive every session across all pages; prints id + new archived flag.
        # Paginates with nextPageToken; skips already-archived sessions.
        limit="${2:-100}"
        token=""
        total=0
        archived=0
        while :; do
            page="$(api GET "/sessions?pageSize=$limit${token:+&pageToken=$token}")"
            die_on_error "$page"
            ids="$(printf '%s' "$page" | jq -r '.sessions[]?.id')"
            [ -z "$ids" ] && break
            while IFS= read -r sid; do
                total=$((total + 1))
                is_archived="$(api GET "/sessions/$sid" | jq -r '.archived // false')"
                if [ "$is_archived" = "true" ]; then
                    continue
                fi
                api POST "/sessions/$sid:archive" > /dev/null 2>&1 || echo "  !! failed: $sid" >&2
                archived=$((archived + 1))
                echo "  archived $sid"
            done <<< "$ids"
            token="$(printf '%s' "$page" | jq -r '.nextPageToken // empty')"
            [ -z "$token" ] && break
        done
        echo "Done: $total sessions seen, $archived newly archived."
        ;;
    curl)
        method="${1:-GET}"
        path="${2:-}"
        if [ -z "$path" ]; then
            echo "ERROR: usage: scripts/jules.sh curl <METHOD> <path> [--data '<json>']" >&2
            exit 1
        fi
        shift 2
        api "$method" "$path" "${@:-}" | jq . || true
        ;;
    help|--help|-h)
        sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'
        ;;
    *)
        echo "ERROR: unknown command '$CMD'. Run scripts/jules.sh help" >&2
        exit 1
        ;;
esac
