#!/usr/bin/env bash
# This experiment is also bundled with its reference lesson.
show_args() {
  printf 'argc=%s\n' "$#"
  printf '<%s>\n' "$@"
}
name='quarterly report.txt'
show_args "$name"
# argc=1; <quarterly report.txt>
show_args $name
# argc=2; <quarterly> then <report.txt>
name=''
show_args "$name"
# argc=1; <>
show_args $name
# argc=0; printf still prints <> for its missing format operand
printf '%s\n' '$name' "$name"
# literal $name followed by one empty line
