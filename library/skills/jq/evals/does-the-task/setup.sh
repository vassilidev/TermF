#!/bin/sh
set -e
cat > releases.json <<'EOF'
[
  {"tag": "v2.1.0", "draft": false, "assets": [{"name": "app.tar.gz", "size": 1048576}, {"name": "app.zip", "size": 2097152}]},
  {"tag": "v2.2.0-rc.1", "draft": true, "assets": []},
  {"tag": "v2.0.3", "draft": false, "assets": []}
]
EOF
