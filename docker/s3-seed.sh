#!/bin/sh
# Demo buckets for stu. Runs in the aws-cli image against the sandbox S3 server.
set -eu
s3() { aws --endpoint-url "$S3_ENDPOINT" s3 "$@"; }

i=0
until s3 ls >/dev/null 2>&1; do
  i=$((i + 1))
  [ "$i" -lt 30 ] || { echo "S3 not ready at $S3_ENDPOINT" >&2; exit 1; }
  sleep 1
done

d=$(mktemp -d)
cd "$d"

mkdir -p docs/guides
cat > docs/README.md <<'MD'
# Docs bucket

Sample objects for **stu**. Open one to preview it, `?` lists the bindings.
MD
printf '# Getting started\n\n1. Run `vimify apply`.\n2. Open `stu`.\n' > docs/guides/getting-started.md
printf '# Keybindings\n\n| Key | Action |\n|-----|--------|\n| j   | down   |\n| k   | up     |\n' > docs/guides/keybindings.md
printf 'Plain text, no extension handling.\n' > docs/notes.txt

# Enough keys under one prefix to scroll and jump.
for day in $(seq -w 1 30); do
  mkdir -p "logs/app/2026/11/$day"
  for h in 00 06 12 18; do
    printf '2026-11-%sT%s:00:00Z INFO request served in %sms\n' "$day" "$h" "$day" \
      > "logs/app/2026/11/$day/$h.log"
  done
done
mkdir -p logs/worker
printf 'ERROR job 42 failed: timeout\n' > logs/worker/errors.log

mkdir -p data/nested/deeper/deepest
printf 'id,name,lang\n1,ada,rust\n2,linus,c\n3,grace,cobol\n' > data/users.csv
printf '{"keys": {"up": "k", "down": "j"}, "enabled": true, "retries": 3}\n' > data/config.json
printf 'report:\n  total: 3\n  failed: 0\n' > data/report.yaml
printf 'bottom of the tree\n' > data/nested/deeper/deepest/leaf.txt
head -c 65536 /dev/urandom > data/blob.bin
# Large enough that a preview has to page.
seq 1 20000 | sed 's/^/line /' > data/large.txt

for b in docs logs data archive; do
  s3 mb "s3://$b" >/dev/null 2>&1 || true
done
for b in docs logs data; do
  s3 cp --recursive --quiet "$b" "s3://$b"
done
echo "seeded: docs logs data archive (empty)"
