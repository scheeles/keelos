# mTLS Certificate Rotation

KeelOS uses mutual TLS (mTLS) to secure all communication between the `osctl` CLI and the `keel-agent` running on nodes. This guide explains how certificate management works and how to use the automatic rotation feature.

## Table of Contents

- [Overview](#overview)
- [Certificate Architecture](#certificate-architecture)
- [Automatic Rotation](#automatic-rotation)
- [Manual Certificate Management](#manual-certificate-management)
- [Client Enrollment](#client-enrollment)
- [Best Practices](#best-practices)
- [Troubleshooting](#troubleshooting)

---

## Overview

### What is mTLS?

Mutual TLS (mTLS) is a security protocol where both the client and server authenticate each other using X.509 certificates. This ensures:

1. **Server Authentication** - Clients verify they're connecting to a legitimate KeelOS node
2. **Client Authentication** - The node verifies the client has proper credentials
3. **Encryption** - All communication is encrypted in transit

### Why Automatic Rotation?

Certificates have a limited validity period to reduce the impact of compromised credentials. KeelOS automatically rotates server certificates before they expire, ensuring:

- **No downtime** from expired certificates
- **Reduced operational burden** - no manual certificate renewal
- **Enhanced security** - regular key rotation limits exposure

---

## Certificate Architecture

### Certificate Hierarchy

```
┌─────────────────────────┐
│   Root CA Certificate   │
│   (10 year validity)    │
└────────────┬────────────┘
             │ signs
             ▼
┌─────────────────────────┐
│  Server Certificate     │
│  (90 day validity)      │
│  CN: keel-agent         │
└─────────────────────────┘
```

### File Locations

All certificate files are stored in `/etc/keel/crypto/`:

| File | Purpose | Permissions |
|------|---------|-------------|
| `ca.pem` | Root CA certificate (public) | `644` (`root:root`) |
| `ca.key` | Root CA private key | `600` (`root:root`) |
| `server.pem` | Server certificate (public) | `644` (`root:root`) |
| `server.key` | Server private key | `600` (`root:root`) |

> **Security Note:** Private keys (`*.key`) should never be readable by non-root users.

### Certificate Properties

#### Root CA Certificate
- **Algorithm:** ECDSA P-256
- **Validity:** 10 years (900 days)
- **Usage:** Certificate signing only
- **Generated:** Automatically on first boot if not present

#### Server Certificate
- **Algorithm:** ECDSA P-256
- **Validity:** 90 days
- **Common Name:** `keel-agent`
- **Usage:** Server authentication, client authentication
- **Rotation Threshold:** 30 days before expiry

---

## Automatic Rotation

### How It Works

The `keel-agent` includes a background certificate manager that:

1. **Checks expiry daily** - Monitors the server certificate validity
2. **Rotates proactively** - Issues a new certificate 30 days before expiry
3. **Atomic replacement** - New certificate is written to `.new` files, verified, then atomically renamed
4. **No downtime** - Current connections continue; new connections use the new certificate

### Rotation Timeline

```
Certificate Issued               Rotation Triggered              Certificate Expires
|                                |                                |
Day 0                           Day 60                          Day 90
├────────────────────────────────┼────────────────────────────────┤
        60 days normal operation        30-day rotation window
```

### Monitoring Rotation

Check when the next rotation will occur:

```bash
osctl cert status
```

Example output:
```
🔐 Certificate Status

  Common Name: keel-agent
  Not Before:  2026-01-27T20:14:00Z
  Not After:   2026-04-27T20:14:00Z
  Days Until Expiry: 89 days
  ✅ Certificate is valid
```

### Rotation Logs

The agent logs all rotation activities:

```bash
# View rotation logs
journalctl -u keel-agent | grep -i "certif"
```

Example log entries:
```
[INFO] Certificate manager initialized successfully
[DEBUG] Checking certificate expiry
[DEBUG] Certificate expiry status: days_until_expiry=89 is_expiring_soon=false
[INFO] Certificate expiring soon, rotating: days_until_expiry=25
[INFO] Rotating server certificate
[INFO] Server certificate rotated successfully
```

---

## Manual Certificate Management

### Check Certificate Status

View detailed information about the current certificate:

```bash
osctl cert status
```

This displays:
- Common name
- Validity period (not before / not after)
- Days until expiry
- Warning if expiring soon (< 30 days)

### Manually Trigger Rotation

Force an immediate certificate rotation:

```bash
osctl cert renew
```

Use cases for manual rotation:
- **Security incident** - Rotate certificates immediately if compromise is suspected
- **Testing** - Verify rotation mechanism works correctly
- **Before maintenance** - Ensure fresh certificates before long maintenance windows

> **Note:** Manual rotation does not reset the validity period to 90 days; it issues a new certificate with 90 days validity from the current time.

### Retrieve CA Certificate

Download the CA certificate for client enrollment:

```bash
# Print to stdout
osctl cert get-ca

# Save to file
osctl cert get-ca --output ca.pem
```

---

## Client Enrollment

To allow a new client (running `osctl`) to connect to a KeelOS node:

### Step 1: Get the CA Certificate

From a trusted source (ideally during initial provisioning):

```bash
osctl --endpoint https://node.example.com:50051 cert get-ca --output ca.pem
```

### Step 2: Generate Client Certificate

> **Future Enhancement:** This will be automated in a future release. Currently, you must manually generate client certificates using the CA private key.

For now, use the CA to issue client certificates:

```bash
# This requires access to ca.key on the node
# In production, use a secure certificate issuance workflow
```

### Step 3: Configure osctl

Place certificates in the expected locations:

```bash
mkdir -p ~/.keel/
cp ca.pem ~/.keel/ca.pem
cp client.pem ~/.keel/client.pem
cp client.key ~/.keel/client.key
chmod 600 ~/.keel/client.key
```

`osctl` will automatically use these certificates when connecting.

---

## Best Practices

### 1. Monitor Certificate Expiry

Set up monitoring to alert if certificates are within 7 days of expiry (rotation should have already occurred by then):

```bash
# Check expiry and exit with error if < 7 days
osctl cert status | grep "Days Until Expiry" | awk '{if ($4 < 7) exit 1}'
```

### 2. Backup CA Certificate

The CA certificate and private key should be backed up securely:

```bash
# Backup CA (secure this backup!)
sudo tar -czf keel-ca-backup-$(date +%Y%m%d).tar.gz \
  /etc/keel/crypto/ca.pem \
  /etc/keel/crypto/ca.key

# Store in a secure location (HSM, vault, encrypted storage)
```

> **Critical:** The CA private key is required to issue new certificates. If lost, all clients must be re-enrolled with a new CA.

### 3. Audit Certificate Changes

Enable audit logging to track certificate operations:

```bash
# View certificate-related operations
journalctl -u keel-agent | grep -E "(renew|rotate|certificate)"
```

### 4. Test Rotation

Periodically test manual rotation in a non-production environment:

```bash
# Force rotation
osctl cert renew

# Verify connectivity still works
osctl status
```

### 5. Use Short-Lived Certificates

The default 90-day validity is a security best practice. Avoid extending this unless absolutely necessary.

---

## Troubleshooting

### Certificate Expired

**Symptom:** `osctl` commands fail with TLS errors

**Cause:** Server certificate expired (rotation didn't occur)

**Solution:**
```bash
# Check certificate status on the node
sudo journalctl -u keel-agent -n 100 | grep cert

# Manual rotation (requires node access)
osctl cert renew

# If rotation fails, check logs
sudo journalctl -u keel-agent -f
```

### Rotation Not Happening

**Symptom:** Certificate is < 30 days from expiry but hasn't rotated

**Cause:** Certificate manager may not be running

**Solution:**
```bash
# Check if agent is running
systemctl status keel-agent

# Restart agent to reinitialize cert manager
sudo systemctl restart keel-agent

# Monitor logs
sudo journalctl -u keel-agent -f | grep cert
```

### CA Certificate Missing

**Symptom:** Agent fails to start with "CA certificate not found"

**Cause:** `/etc/keel/crypto/ca.pem` doesn't exist

**Solution:**

The agent will automatically generate a new CA on startup. However, this will invalidate all existing client certificates.

```bash
# Restart agent to generate new CA
sudo systemctl restart keel-agent

# Re-enroll all clients with new CA
osctl cert get-ca --output new-ca.pem
```

### Manual Certificate Rotation Fails

**Symptom:** `osctl cert renew` returns an error

**Possible Causes:**
1. **Disk full** - Check `/etc/keel/crypto/` has space
2. **Permission denied** - Ensure `/etc/keel/crypto/` is writable by root
3. **CA not initialized** - Check agent logs for CA errors

**Solution:**
```bash
# Check disk space
df -h /etc/keel

# Check permissions
ls -la /etc/keel/crypto/

# Check agent logs
sudo journalctl -u keel-agent -n 50
```

### Client Connection Fails After Rotation

**Symptom:** `osctl` worked before, now fails with certificate errors

**Cause:** Client may be using an old CA certificate

**Solution:**
```bash
# Get updated CA certificate
osctl cert get-ca --output ca.pem

# Replace old CA
mv ca.pem ~/.keel/ca.pem

# Test connection
osctl status
```

---

## Advanced Configuration

### Custom Certificate Validity

To change the certificate validity period, modify the agent configuration:

> **Note:** This feature is planned for a future release. Currently, validity periods are hardcoded (90 days for server certificates, 900 days for CA).

### Disable Automatic Rotation

Not recommended for production, but useful for testing:

> **Note:** This feature is planned for a future release.

---

## Security Considerations

### Threat Model

The certificate rotation system protects against:

1. **Certificate expiry** - Automatic renewal prevents outages
2. **Key compromise** - Regular rotation limits exposure window
3. **Unauthorized access** - mTLS ensures only trusted clients connect

### Current Limitations

> **Important:** These limitations will be addressed in future releases:

1. **No Certificate Revocation** - Compromised certificates remain valid until expiry
   - *Mitigation:* Use short validity periods (90 days)
   
2. **Manual Client Enrollment** - Client certificates must be manually issued
   - *Mitigation:* Secure CA private key access
   
3. **No TPM Integration** - Private keys stored in filesystem
   - *Mitigation:* File permissions, disk encryption

### Future Enhancements

See [Issue #15 - Security Enhancements](https://github.com/scheeles/keel/issues/15) for planned improvements:

- Certificate pinning
- TPM-backed key storage
- Certificate Revocation Lists (CRL)
- Automated client enrollment
- Secure boot integration

---

## API Reference

For programmatic access to certificate management:

- [RenewCertificate RPC](../reference/api.md#renewcertificate)
- [GetCertificateInfo RPC](../reference/api.md#getcertificateinfo)
- [GetCACertificate RPC](../reference/api.md#getcacertificate)

## CLI Reference

For detailed `osctl cert` command usage:

- [osctl cert commands](../reference/osctl.md#cert-commands)
