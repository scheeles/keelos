# Certificate Management - Quick Reference

## Commands

### Bootstrap Certificates (Development)
```bash
# Generate new bootstrap certificate
osctl init bootstrap

# Valid for 24 hours
# Location: /var/lib/keel/crypto/trusted-clients/bootstrap/<hash>.pem
```

### Certificate Status
```bash
# Check certificate expiry (via openssl)
openssl x509 -in /var/lib/keel/crypto/operational.pem -noout -enddate

# Check auto-renewal status (via logs)
journalctl -u keel-agent | grep -i renewal

# Trigger manual rotation (via RPC)
grpcurl -plaintext localhost:50051 keel.v1.NodeService/RotateCertificate
```

## Metrics

### Prometheus Queries
```promql
# Days until expiry
keel_certificate_days_remaining

# Certificate expiry timestamp
keel_certificate_expiry_timestamp

# Renewal success rate
rate(keel_certificate_renewals_success[1h])

# Renewal errors
rate(keel_certificate_renewals_errors[1h])
```

### Grafana Alerts
```yaml
# Alert when < 7 days remaining
keel_certificate_days_remaining < 7

# Alert on renewal errors
keel_certificate_renewals_errors > 0
```

## File Locations

```
/var/lib/keel/crypto/
├── operational.pem           # K8s-signed cert (365d)
├── operational.key           # Private key
├── operational.pem.backup    # Previous cert
└── trusted-clients/
    └── bootstrap/
        └── <hash>.pem        # Bootstrap cert (24h)

/etc/keel/crypto/
├── server.pem                # Agent server cert
├── server.key                # Agent server key
└── ca.pem                    # Kubernetes CA
```

## Auto-Renewal Timeline

```
Day 0:    Certificate created (expires Day 365)
Day 1-335: Check daily, no action
Day 336:   29 days remaining → OK
Day 337:   28 days remaining → RENEW!
          ├─ Backup old cert
          ├─ Request new cert (K8s CSR)
          ├─ Write new cert (expires Day 702)
          └─ Update metrics
Day 338+:  Check daily, no action
```

## Troubleshooting

### Bootstrap Certificate Expired
```bash
# Solution: Generate new one
osctl init bootstrap
```

### Auto-Renewal Failing
```bash
# Check logs
journalctl -u keel-agent -f | grep renewal

# Verify K8s access
kubectl get csr | grep keel

# Check RBAC
kubectl auth can-i create certificatesigningrequests \
  --as=system:serviceaccount:default:keel-agent
```

### Metrics Not Showing
```bash
# Check OTLP endpoint
echo $OTLP_ENDPOINT

# Verify agent logs
journalctl -u keel-agent | grep "metrics updated"
```

## Configuration

### Current Settings (Hardcoded)
- **Renewal Threshold:** 30 days before expiry
- **Check Interval:** Every 24 hours
- **Bootstrap Validity:** 24 hours
- **Operational Validity:** 365 days

### Future (Configurable via /etc/keel/agent.toml)
```toml
[certificates]
auto_renewal_enabled = true
renewal_threshold_days = 30
check_interval_hours = 24
```

## RBAC Setup

```bash
# Apply required permissions
kubectl apply -f k8s/rbac.yaml

# Verify
kubectl get serviceaccount keel-agent
kubectl get clusterrolebinding keel-agent-csr
```

## Environment Variables

```bash
# Required for K8s integration
export NODE_NAME=worker-1

# Optional: OpenTelemetry endpoint
export OTLP_ENDPOINT=http://jaeger:4317
```

## Common Scenarios

### New Node Setup
```bash
1. Deploy agent with RBAC
2. Agent auto-creates operational cert
3. Auto-renewal daemon starts
4. Metrics exported to OTLP
```

### Manual Rotation
```bash
grpcurl -plaintext localhost:50051 \
  keel.v1.NodeService/RotateCertificate
```

### Certificate Backup/Restore
```bash
# Backup created automatically before renewal
# Restore from backup:
cp /var/lib/keel/crypto/operational.pem.backup \
   /var/lib/keel/crypto/operational.pem
   
systemctl restart keel-agent
```

## See Also

- [Full Certificate Management Guide](certificate-management.md)
- [K8s Integration](../k8s/README.md)
- [RBAC Manifests](../k8s/rbac.yaml)
