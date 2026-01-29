# Certificate Management Guide

## Overview

KeelOS provides a comprehensive certificate management system with automatic renewal, dual-CA support, and OpenTelemetry monitoring. The system supports both development/bootstrap certificates and production Kubernetes-signed certificates.

## Quick Start

### Development Setup (Bootstrap Certificates)

Bootstrap certificates are self-signed, short-lived (24 hours) certificates for development and initial setup.

```bash
# Initialize bootstrap certificate on a node
osctl init bootstrap

# Output:
# ✓ Bootstrap certificate generated
#   Cert: /var/lib/keel/crypto/trusted-clients/bootstrap/<hash>.pem
#   Valid for: 24 hours
```

The certificate is automatically loaded by `osctl` for subsequent commands.

### Production Setup (Kubernetes Operational Certificates)

Operational certificates are Kubernetes-signed, long-lived (365 days) certificates with automatic renewal.

**Prerequisites:**
- KeelOS agent running on Kubernetes nodes
- RBAC permissions configured
- `NODE_NAME` environment variable set

**Setup:**

1. **Apply RBAC manifests:**
```bash
kubectl apply -f k8s/rbac.yaml
```

2. **Deploy agent with environment variables:**
```yaml
env:
- name: NODE_NAME
  valueFrom:
    fieldRef:
      fieldPath: spec.nodeName
```

3. **Agent auto-initializes certificates on startup:**
```
INFO K8s operational certificates initialized:
  Cert: /var/lib/keel/crypto/operational.pem
  Key: /var/lib/keel/crypto/operational.key
  
INFO Certificate auto-renewal enabled (threshold: 30 days, check interval: 24 hours)
```

## Certificate Types

### Bootstrap Certificates

| Property | Value |
|----------|-------|
| **Validity** | 24 hours |
| **Signing** | Self-signed by agent |
| **Location** | `/var/lib/keel/crypto/trusted-clients/bootstrap/` |
| **Auto-Renewal** | No (recreate manually) |
| **Use Case** | Development, initial setup |

**When to use:**
- Local development
- Testing
- Initial node setup before K8s integration

### Operational Certificates

| Property | Value |
|----------|-------|
| **Validity** | 365 days |
| **Signing** | Kubernetes CA |
| **Location** | `/var/lib/keel/crypto/operational.pem` |
| **Auto-Renewal** | Yes (30 days before expiry) |
| **Use Case** | Production Kubernetes clusters |

**When to use:**
- Production deployments
- Kubernetes-managed nodes
- Long-running systems

## Auto-Renewal

### How It Works

The auto-renewal daemon runs continuously in the background:

1. **Check every 24 hours** (configurable)
2. **Parse certificate expiry** from X.509 metadata
3. **If < 30 days remaining** → trigger renewal
4. **Backup old certificate** to `.backup` file
5. **Request new certificate** via Kubernetes CSR API
6. **Write new certificate** and key
7. **Update metrics** and log success

### Configuration

Currently hardcoded (future: configurable via `/etc/keel/agent.toml`):

```toml
[certificates]
auto_renewal_enabled = true
renewal_threshold_days = 30  # Renew when < 30 days remain
check_interval_hours = 24     # Check once per day
```

### Manual Trigger

Auto-renewal happens automatically, but you can also trigger rotation via RPC:

```bash
# Via grpcurl (manual rotation RPC)
grpcurl -plaintext \
  -d '{"force": true}' \
  localhost:50051 \
  keel.v1.NodeService/RotateCertificate
```

Response:
```json
{
  "success": true,
  "message": "Certificate rotated successfully",
  "certPath": "/var/lib/keel/crypto/operational.pem",
  "expiresAt": "2027-01-29T16:00:00Z"
}
```

## Monitoring

### OpenTelemetry Metrics

The following metrics are exported via OTLP:

#### Certificate Expiry Metrics

**`keel.certificate.expiry_timestamp`** (Observable Gauge)
- Certificate expiry time as Unix timestamp
- Updates on every renewal check
- Type: int64

**`keel.certificate.days_remaining`** (Observable Gauge)
- Days until certificate expires
- Calculated from expiry timestamp
- Type: int64

#### Renewal Metrics

**`keel.certificate.renewals.success`** (Counter)
- Count of successful renewals
- Labels: `cert_type="operational"`
- Type: uint64

**`keel.certificate.renewals.errors`** (Counter)
- Count of failed renewal attempts
- Labels: `cert_type`, `error`
- Type: uint64

### Prometheus Queries

```promql
# Days until certificate expires
keel_certificate_days_remaining

# Certificate expiry timestamp
keel_certificate_expiry_timestamp

# Renewal rate (last 5 minutes)
rate(keel_certificate_renewals_success[5m])

# Error rate
rate(keel_certificate_renewals_errors[5m])

# Time until expiry (human-readable)
(keel_certificate_expiry_timestamp - time()) / 86400
```

### Grafana Dashboard

Example panel queries:

**Certificate Expiry Countdown:**
```promql
keel_certificate_days_remaining
```

**Renewal History:**
```promql
increase(keel_certificate_renewals_success[1h])
```

**Error Alerts:**
```promql
keel_certificate_renewals_errors > 0
```

### Setup OTLP Export

```bash
# Set OTLP endpoint for metrics/tracing
export OTLP_ENDPOINT=http://jaeger:4317

# Or in Kubernetes
env:
- name: OTLP_ENDPOINT
  value: "http://jaeger:4317"
```

## Troubleshooting

### Bootstrap Certificate Expired

**Symptom:**
```
Error: certificate has expired or is not yet valid
```

**Solution:**
```bash
# Generate new bootstrap certificate
osctl init bootstrap
```

### Operational Certificate Not Created

**Symptom:**
```
INFO No operational certificate found, skipping renewal check
```

**Possible Causes:**

1. **Not running in Kubernetes:**
```bash
# Check for K8s service account token
ls /var/run/secrets/kubernetes.io/serviceaccount/token
```

2. **Missing NODE_NAME:**
```bash
# Check environment variable
echo $NODE_NAME
```

3. **RBAC permissions missing:**
```bash
# Verify ServiceAccount exists
kubectl get serviceaccount keel-agent

# Verify ClusterRoleBinding
kubectl get clusterrolebinding keel-agent-csr
```

**Solution:**
```bash
# Apply RBAC manifests
kubectl apply -f k8s/rbac.yaml

# Ensure NODE_NAME is set
export NODE_NAME=worker-1

# Restart agent
systemctl restart keel-agent
```

### Auto-Renewal Failing

**Symptom:**
```
ERROR Certificate renewal check failed: Failed to request certificate
```

**Debug Steps:**

1. **Check agent logs:**
```bash
journalctl -u keel-agent -f | grep -i renewal
```

2. **Verify K8s API access:**
```bash
# From agent pod/host
kubectl get certificatesigningrequests
```

3. **Check RBAC permissions:**
```bash
kubectl auth can-i create certificatesigningrequests \
  --as=system:serviceaccount:default:keel-agent
```

4. **View CSR status:**
```bash
kubectl get csr | grep keel
```

**Common Fixes:**

- Ensure ClusterRole has `approve` verb for CSRs
- Check network connectivity to K8s API
- Verify certificate not corrupted:
  ```bash
  openssl x509 -in /var/lib/keel/crypto/operational.pem -text -noout
  ```

### Metrics Not Appearing

**Symptom:**
Metrics not visible in Prometheus/Grafana

**Checks:**

1. **OTLP endpoint configured:**
```bash
echo $OTLP_ENDPOINT
```

2. **Agent logs show metrics:**
```bash
journalctl -u keel-agent | grep "metrics updated"
```

3. **OTLP collector receiving data:**
```bash
# Check collector logs
kubectl logs -l app=jaeger
```

## Architecture

### Certificate Lifecycle

```
┌─────────────────────────────────────────────────┐
│  Bootstrap Certificate (Development)            │
├─────────────────────────────────────────────────┤
│  1. osctl init bootstrap                        │
│  2. Agent generates self-signed cert (24h)      │
│  3. Stored in /var/lib/keel/crypto/bootstrap/   │
│  4. Manual renewal (re-run command)             │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│  Operational Certificate (Production)           │
├─────────────────────────────────────────────────┤
│  1. Agent starts in K8s cluster                 │
│  2. Checks for existing cert                    │
│  3. If missing: K8s CSR workflow ───────┐       │
│     ├─ Generate key pair               │       │
│     ├─ Create CSR                       │       │
│     ├─ Submit to K8s API                │       │
│     ├─ Auto-approve (requires RBAC)     │       │
│     ├─ Wait for K8s to sign             │       │
│     └─ Retrieve signed cert             │       │
│  4. Store cert/key (365d validity)      │       │
│  5. Auto-renewal daemon starts <────────┘       │
│     └─ Check every 24h                          │
│        └─ Renew if < 30 days remaining          │
└─────────────────────────────────────────────────┘
```

### Dual-CA mTLS

The agent supports clients authenticated by either CA:

```
┌──────────────────┐
│   keel-agent     │
│   (gRPC Server)  │
├──────────────────┤
│  TLS Manager     │
│  ├─ Bootstrap CA │  ──> Validates bootstrap certs
│  └─ K8s CA       │  ──> Validates operational certs
└──────────────────┘
         ▲
         │
    ┌────┴────┐
    │         │
┌───┴───┐ ┌──┴────┐
│ osctl │ │ osctl │
│(boot) │ │ (ops) │
└───────┘ └───────┘
```

### File Structure

```
/var/lib/keel/crypto/
├── operational.pem           # K8s-signed certificate (365d)
├── operational.key           # Private key
├── operational.pem.backup    # Previous cert (after renewal)
├── operational.key.backup    # Previous key
└── trusted-clients/
    └── bootstrap/
        └── <hash>.pem        # Self-signed cert (24h)

/etc/keel/crypto/
├── server.pem                # Agent server certificate
├── server.key               # Agent server key
└── ca.pem                   # Kubernetes CA (operational)
```

## Security Considerations

### Private Key Protection

- Private keys **NEVER transmitted** over the network
- Generated locally on each node
- Stored with `0600` permissions
- Backup keys preserved for rollback

### RBAC Principle of Least Privilege

The `keel-agent` ServiceAccount only has permissions for:
- Creating CSRs
- Approving own CSRs
- Reading CSR status
- Deleting old CSRs

**Not granted:**
- Access to Secrets
- Access to other node certificates
- Cluster-wide certificate signing

### Certificate Validation

- X.509 certificate validation on every connection
- Expiry checking before acceptance
- Subject name verification
- CA chain validation

## Best Practices

### Development

1. **Use bootstrap certificates** for local testing
2. **Regenerate daily** (24h validity)
3. **Don't commit** certificates to version control

### Production

1. **Always use operational certificates** in K8s
2. **Monitor expiry metrics** in Grafana
3. **Set up alerts** for renewal failures
4. **Test rotation** in staging before production
5. **Keep RBAC manifests** in version control

### Monitoring

1. **Alert on `days_remaining < 7`** (safety margin)
2. **Alert on `renewals.errors > 0`**
3. **Dashboard showing expiry countdown**
4. **Track renewal success rate**

## Examples

### Complete Production Setup

```bash
# 1. Apply RBAC
kubectl apply -f k8s/rbac.yaml

# 2. Deploy agent with manifests
kubectl apply -f k8s/agent-daemonset.yaml

# 3. Wait for certificates
kubectl logs -l app=keel-agent | grep "operational certificates"

# 4. Verify metrics
curl http://localhost:4317/metrics | grep keel_certificate

# 5. Test connection with osctl
osctl status
```

### Monitoring Setup

```yaml
# prometheus-rules.yaml
groups:
- name: keel_certificates
  rules:
  - alert: CertificateExpiringSoon
    expr: keel_certificate_days_remaining < 7
    annotations:
      summary: "Certificate expires in {{ $value }} days"
      
  - alert: CertificateRenewalFailing
    expr: rate(keel_certificate_renewals_errors[5m]) > 0
    annotations:
      summary: "Certificate renewal errors detected"
```

## FAQ

**Q: Can I disable auto-renewal?**  
A: Not currently configurable. Auto-renewal is essential for production operation.

**Q: What happens if renewal fails?**  
A: The old certificate remains valid until expiry. The daemon retries every 24 hours.

**Q: Can I use custom CAs?**  
A: Operational certificates must be Kubernetes-signed. Bootstrap certificates are always self-signed.

**Q: How do I rotate certificates manually?**  
A: Use the `RotateCertificate` RPC (requires gRPC client). Future: `osctl rotate` command.

**Q: Do I need to restart the agent after renewal?**  
A: Currently yes (certificate hot-reload not yet implemented). The new cert is used on next restart.

## Related Documentation

- [K8s Integration Guide](../k8s/README.md)
- [RBAC Configuration](../k8s/rbac.yaml)
- [OpenTelemetry Setup](telemetry.md)
- [Security Architecture](security.md)
