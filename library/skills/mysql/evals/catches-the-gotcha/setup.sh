#!/bin/sh
set -e
mkdir -p migrations
cat > my.cnf <<'EOF'
[mysqld]
default_authentication_plugin=mysql_native_password
character-set-server=utf8mb4
EOF
cat > migrations/0002_app_user.sql <<'EOF'
CREATE USER 'app'@'%' IDENTIFIED WITH mysql_native_password BY 'change-me';
GRANT SELECT, INSERT, UPDATE, DELETE ON shop.* TO 'app'@'%';
EOF
