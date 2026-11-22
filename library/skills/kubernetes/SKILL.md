---
name: kubernetes
description: >-
  Writes and fixes Kubernetes manifests, Kustomize overlays and Helm charts so they validate and run safely:
  Deployments, Services, probes, resource requests, security contexts, disruption budgets, sidecars, routing
  with Gateway API instead of Ingress NGINX, API versions removed by upgrades. Use when a task touches YAML with
  apiVersion and kind, kustomization.yaml, Chart.yaml, values.yaml or templates/, or kubectl and helm commands.
  Not for Dockerfiles or Compose files (docker), nor for the cloud resources around a cluster (terraform, cloud).
license: MIT
metadata:
  termf-stacks: "kubernetes helm"
  termf-roles: "devops"
  termf-versions: ">=1.33"
  termf-verified: "2026-11-22 kubernetes@1.37.2 helm@4.3.2 gateway-api@1.7.0 kubeconform@0.8.0"
  termf-sources: "https://kubernetes.io/releases/ https://kubernetes.io/blog/2026/08/26/kubernetes-v1-37-release/ https://kubernetes.io/docs/reference/using-api/deprecation-guide/ https://kubernetes.io/blog/2025/11/11/ingress-nginx-retirement/ https://kubernetes.io/docs/concepts/services-networking/ingress/ https://kubernetes.io/docs/concepts/security/pod-security-standards/ https://gateway-api.sigs.k8s.io/ https://github.com/helm/helm/releases/tag/v4.0.0 https://github.com/yannh/kubeconform"
---
# Kubernetes

No official agent skill exists for Kubernetes itself (cloud vendors ship skills for their managed clusters: see the `cloud` skill). The docs at kubernetes.io are versioned; read the page of the target minor.

1. Find the target cluster version: the repository's CI, Terraform or cluster config (`kubernetes_version`, `--kubernetes-version`), or `kubeVersion` in `Chart.yaml`. Default to the oldest supported minor (1.35 on 2026-11-22) when nothing says. Done when you can name the version you validate against.
2. Find how manifests are produced: plain YAML, Kustomize (`kustomization.yaml`), Helm (`Chart.yaml`), or a generator. Change the source, never the rendered output. Done when you know which file renders each object you change.
3. Check the context before any command that talks to a cluster: `kubectl config current-context`. Work offline (render, validate) unless the task names a local cluster (kind, k3d, minikube) and that is the current context. Done when every command you plan is offline or aimed at that local cluster.
4. Write each workload with these defaults, keeping any pattern the repository already applies:
   - An image pinned by tag and digest, never `:latest`, so a rollout is reproducible.
   - `resources.requests` for CPU and memory on every container, and a memory limit, so the scheduler and the OOM killer behave predictably.
   - A `readinessProbe` that reflects whether this pod can serve now; a `livenessProbe` only for a deadlock the process cannot leave, never a check of a dependency (a database outage would restart every pod); a `startupProbe` for slow starts.
   - The `restricted` Pod Security settings: `runAsNonRoot: true`, `allowPrivilegeEscalation: false`, `capabilities: {drop: [ALL]}`, `seccompProfile: {type: RuntimeDefault}`, plus `readOnlyRootFilesystem: true` with an `emptyDir` for scratch paths.
   - A `PodDisruptionBudget` for anything with more than one replica.
   Done when the rendered objects pass `kubeconform -strict`.
5. Render and validate: `kustomize build` or `kubectl kustomize`, or `helm template`, piped to `kubeconform`. Done when the commands of `## Verify` pass.
6. Put each command with its result in your report, and say which objects need a live cluster to be checked (admission policies, CRDs, quotas). Done when the report lists them, or says there are none.

## Gotchas

- Ingress NGINX (the community controller `kubernetes/ingress-nginx`) is retired: no releases or security fixes after March 2026. The Ingress API itself is frozen, still GA and not scheduled for removal. For new routing, write Gateway API objects (`Gateway`, `HTTPRoute`; current release v1.7.0); for an existing Ingress, keep the controller the cluster runs and name the migration in your report instead of rewriting it unasked.
- Supported minors on 2026-11-22: 1.37 (latest 1.37.2), 1.36 and 1.35; 1.34 reached end of life on 2026-10-27. Check `apiVersion`s against the deprecation guide for the target version, not from memory.
- Native sidecars are stable since 1.33: an `initContainers` entry with `restartPolicy: Always` starts before and stops after the app containers. Use it for proxies and log shippers instead of a plain second container, which races the app at start and blocks Job completion.
- Helm 4 (November 2025) changed some CLI flags and output and adds server-side apply; charts with `apiVersion: v2` still work. Read `helm version` before copying flags from Helm 3 examples; Helm 3 still gets fixes (3.22.x).
- `kubeconform` needs schemas for custom resources: pass `-schema-location default -schema-location '<CRD catalog URL template>'` or the repository's own schemas, and use `-ignore-missing-schemas` only with the skipped kinds named in your report.
- A `Secret` manifest only base64-encodes values. Use the mechanism the repository has (External Secrets, Sealed Secrets, SOPS) and never commit a plain one.

## Verify

- The project's own targets first (`make lint`, a CI job that renders and validates).
- `kubectl kustomize <dir> | kubeconform -strict -summary -kubernetes-version <version from step 1, e.g. 1.35.0>`.
- `helm lint <chart>` then `helm template <release> <chart> -f <values> | kubeconform -strict -summary -kubernetes-version <version>`
- Only against a local cluster that is the current context: `kubectl apply --dry-run=server -f <file>` and `kubectl diff -f <file>`.

## Security

- Never `kubectl apply`, `delete`, `edit`, `scale`, `rollout restart` or `helm install|upgrade|uninstall` against a shared or production context, and never switch the current context; those are the user's to run.
- Label namespaces with `pod-security.kubernetes.io/enforce: restricted` (or the level the repository uses) so a pod without the security settings above is rejected.
- `hostPath`, `hostNetwork`, `hostPID`, `privileged: true` and added capabilities give access to the node; flag any of them in your report with the reason the workload needs it.
- Give each workload its own ServiceAccount with only the RBAC verbs it uses, and `automountServiceAccountToken: false` when it does not call the API.
- Add a default-deny `NetworkPolicy` per namespace and open only the flows the task needs, when the repository already uses NetworkPolicies.

## References

- Deprecated and removed APIs per version: https://kubernetes.io/docs/reference/using-api/deprecation-guide/
- Pod Security Standards (what `restricted` requires): https://kubernetes.io/docs/concepts/security/pod-security-standards/
- Gateway API: https://gateway-api.sigs.k8s.io/ ; Ingress NGINX retirement: https://kubernetes.io/blog/2025/11/11/ingress-nginx-retirement/
