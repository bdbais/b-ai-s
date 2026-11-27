#!/bin/bash
CLI=${CLI:-../target/release/bais-model-cli}
pass=0; fail=0
for m in "$@"; do
 for p in "the cat sat" "hello, world!" "in the beginning there was" "a" "Hello World 123, it's 4567 cats!  ok"; do
  r=$(./ref "$m" "$p" 32 2>/dev/null)
  b=$($CLI "$m" "$p" 32 2>/dev/null)
  if [ "$r" == "$b" ]; then pass=$((pass+1)); else fail=$((fail+1)); echo "DIFF $m [$p]"; echo " ref: $r"; echo " bai: $b"; fi
 done
done
echo "uguali $pass, diversi $fail"
