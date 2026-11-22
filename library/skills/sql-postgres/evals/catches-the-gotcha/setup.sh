#!/bin/sh
set -e
mkdir -p queries
cat > queries/user_tags.sql <<'EOF'
SELECT u.id,
       JSON_ARRAY(SELECT t.name FROM tags t WHERE t.user_id = u.id) AS tags
FROM users u;
EOF
