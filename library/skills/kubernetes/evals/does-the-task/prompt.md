---
description: Harden a bare Deployment for production.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our security review says this Deployment is not production ready. Fix it and reply with the full manifest plus anything else we should add next to it. The app listens on 8080, exposes /healthz/ready, and needs Postgres to answer requests.

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: orders
  namespace: shop
spec:
  replicas: 3
  selector:
    matchLabels: {app: orders}
  template:
    metadata:
      labels: {app: orders}
    spec:
      containers:
        - name: orders
          image: ghcr.io/acme/orders:latest
          ports:
            - containerPort: 8080
          env:
            - name: DATABASE_URL
              value: postgres://orders:s3cret@db.shop:5432/orders
```
