---
description: A Kubernetes manifest question, which the kubernetes skill covers.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

Our pods get traffic before the app has finished warming its cache. Add the right probes to this Deployment:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: api
spec:
  replicas: 3
  selector:
    matchLabels: {app: api}
  template:
    metadata:
      labels: {app: api}
    spec:
      containers:
        - name: api
          image: ghcr.io/acme/api:1.4.2
          ports:
            - containerPort: 8080
```
