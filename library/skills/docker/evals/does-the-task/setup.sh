#!/bin/sh
set -e
cat > Dockerfile <<'EOF'
FROM node:24
WORKDIR /app
COPY . .
RUN npm ci
RUN npm run build
EXPOSE 3000
CMD ["node", "dist/server.js"]
EOF
cat > package.json <<'EOF'
{ "name": "api", "private": true, "scripts": { "build": "tsc", "start": "node dist/server.js" } }
EOF
