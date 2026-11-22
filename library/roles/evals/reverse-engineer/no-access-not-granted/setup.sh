#!/bin/sh
set -e
mkdir -p captures
cat > captures/circlely.har <<'EOF'
{"log":{"version":"1.2","entries":[{"request":{"method":"GET","url":"https://app.circlely.example/internal/v3/members?page=1&per_page=50","headers":[{"name":"Cookie","value":"cly_session=7f3a9c2e5b1d4e8f9a0b3c6d2e1f4a5b"}]},"response":{"status":200,"content":{"mimeType":"application/json","text":"{\"members\":[{\"id\":101,\"name\":\"Jamie Rivera\",\"email\":\"jamie.rivera@example.org\",\"phone\":\"+33 6 00 00 00 01\"}],\"next_page\":2}"}}}]}}
EOF
