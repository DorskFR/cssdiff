#!/usr/bin/env bash
# Validate cssdiff against Chrome and time both on every mutation in work/.
# usage: bench/run.sh <site-dir> <base> [mut-NN ...]
# RESULTS=path overrides the results file (default work/results.tsv).
set -euo pipefail
cd "$(dirname "$0")"
site=$1 base=$2
shift 2
results=${RESULTS:-work/results.tsv}
if (( $# )); then dirs=("${@/#/work/}"); else dirs=(work/mut-*/); fi
viewports=375x812,768x1024,1440x900
bin=../target/release/cssdiff
$bin --list-properties > work/properties.txt
[[ -f work/chrome-base.json ]] || node chrome-styles.mjs "$site" "$base" $viewports work/properties.txt work/chrome-base.json
if [[ ! -f work/noise.json ]]; then
  node chrome-styles.mjs "$site" "$base" $viewports work/properties.txt work/chrome-base2.json
  python3 -I noise.py work/chrome-base.json work/chrome-base2.json work/noise.json
  rm work/chrome-base2.json
fi

now() { date +%s.%N; }
printf 'mutation\tkind\tcssdiff_s\tchrome_s\tverdict\tchrome\tcssdiff\tboth\n' > "$results"
for dir in "${dirs[@]}"; do
  m=$(basename "$dir")
  t0=$(now)
  set +e; $bin --json -b "$base" -v $viewports "$site" "$dir" > "work/$m.cssdiff.json" 2>/dev/null; code=$?; set -e
  t1=$(now)
  [[ $code -le 1 ]] || { echo "$m: cssdiff exit $code"; exit 1; }
  node chrome-styles.mjs "$dir" "$base" $viewports work/properties.txt "work/$m.chrome.json"
  t2=$(now)
  line=$(python3 -I compare.py work/chrome-base.json "work/$m.chrome.json" "work/$m.cssdiff.json" work/noise.json "work/$m.compare.json")
  rm "work/$m.chrome.json"
  kind=$(python3 -I -c "import json,sys;print(next(x['kind'] for x in json.load(open('work/mutations.json')) if x['name']=='$m'))")
  read -r verdict c o b <<<"$(echo "$line" | sed -E 's/ +/ /g; s/chrome=|cssdiff=|both=//g')"
  printf '%s\t%s\t%.3f\t%.3f\t%s\t%s\t%s\t%s\n' "$m" "$kind" "$(echo "$t1-$t0" | bc)" "$(echo "$t2-$t1" | bc)" "$verdict" "$c" "$o" "$b" | tee -a "$results"
done
