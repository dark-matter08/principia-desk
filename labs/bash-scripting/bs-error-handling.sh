#!/usr/bin/env bash
# This experiment is also bundled with its reference lesson.
# A subshell keeps these option changes out of your interactive shell.
(
  set +e
  set +o pipefail
  false | cat
  printf 'without pipefail: %s\n' "$?"
  set -o pipefail
  false | cat
  printf 'with pipefail: %s\n' "$?"
)
# Expected statuses: 0 then 1.
produce_report() {
  printf '%s\n' 'starting report' >&2
  return 7
}
if produce_report; then
  printf '%s\n' 'publish report'
else
  report_status=$?
  printf 'report failed: %s\n' "$report_status" >&2
fi
# The publish message must never appear.
