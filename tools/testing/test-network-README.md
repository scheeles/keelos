# Network Integration Test

## Overview

`test-network.sh` is a comprehensive integration test for the KeelOS Network Management API. It verifies that network configuration works end-to-end, including IPv4, IPv6, dual-stack, and DNS configurations.

## What It Tests

### Test 1: DHCP Fallback ✓
- Verifies system boots with default DHCP configuration
- Checks that eth0 interface is present and operational
- **Expected**: eth0 interface visible in network status

### Test 2: IPv4 Static Configuration ✓
- Configures static IPv4 address via osctl
- Verifies configuration is saved correctly
- **Command**: `osctl network config set --interface eth0 --ip 192.168.100.10/24 --gateway 192.168.100.1`
- **Expected**: Configuration shows IPv4 address and gateway

### Test 3: IPv6-Only Configuration ✓
- Configures IPv6-only address (no IPv4)
- Verifies IPv6 gateway configuration
- **Command**: `osctl network config set --interface eth0 --ipv6 2001:db8::10/64 --ipv6-gateway 2001:db8::1`
- **Expected**: Configuration shows IPv6 address and gateway

### Test 4: Dual-Stack Configuration ✓
- Configures both IPv4 and IPv6 on same interface
- Tests multiple IPv6 addresses
- **Command**: `osctl network config set --interface eth0 --ip 192.168.100.20/24 --ipv6 2001:db8::20/64 --ipv6 fd00::20/64`
- **Expected**: Configuration shows both IPv4 and IPv6

### Test 5: DNS Configuration ✓
- Configures DNS nameservers (IPv4 and IPv6)
- Configures search domains
- **Command**: `osctl network dns set --nameserver 8.8.8.8 --nameserver 2001:4860:4860::8888 --search example.com`
- **Expected**: DNS configuration saved correctly

### Test 6: Configuration Persistence (Manual)
- Requires manual verification
- Tests that configuration survives reboot
- **Steps**:
  1. Configure network
  2. Reboot system
  3. Verify config applied at boot
  4. Check network status shows configured IPs

## Prerequisites

1. **Build artifacts** must exist:
   - `build/kernel/bzImage`
   - `build/initramfs.cpio.gz`
   - `build/sda.img`
   - `build/osctl`

2. **QEMU** must be installed:
   ```bash
   # macOS
   brew install qemu
   
   # Ubuntu/Debian
   sudo apt-get install qemu-system-x86
   ```

3. **Network port 50052** must be available (for gRPC)

## Running the Test

### Locally
```bash
# Build the project first
./tools/build.sh

# Run the network test
./tools/testing/test-network.sh
```

### In CI
The test runs automatically as part of the CI pipeline:
- Triggered on: PRs to main, pushes to main
- Job name: `test-network`
- Runs in parallel with other integration tests

## Test Output

### Success
```
>>> Starting Network Integration Test...

========================================
TEST 1: DHCP Fallback
========================================
[TEST1] Starting QEMU...
[TEST1] Boot successful in 15s
[TEST1] ✓ Interface eth0 found
TEST 1: PASS

========================================
TEST 2: IPv4 Static Configuration
========================================
[TEST2] Configuring static IPv4...
[TEST2] ✓ Configuration verified
TEST 2: PASS

...

========================================
TEST SUMMARY
========================================

Total Tests: 5
Passed: 5
Failed: 0

>>> ALL TESTS PASSED! ✓
```

### Failure
```
[TEST2] ✗ Configuration check failed
Expected pattern: 192.168.100.10/24
Actual output:
No network configuration found

TEST 2: FAIL

========================================
TEST SUMMARY
========================================

Total Tests: 5
Passed: 1
Failed: 4

>>> SOME TESTS FAILED ✗
```

## Debugging

### View QEMU logs
```bash
tail -f /tmp/qemu-network-test.log
```

### Run osctl manually
```bash
# While QEMU is running
./build/osctl --endpoint localhost:50052 network status
./build/osctl --endpoint localhost:50052 network config show
```

### Increase timeout
Edit `test-network.sh` and change:
```bash
TIMEOUT=90  # Increase to 120 or more
```

## Known Limitations

1. **No actual network connectivity**: QEMU runs with user-mode networking, so configured IPs don't actually work for external connectivity. The test only verifies configuration persistence and API functionality.

2. **No reboot testing**: Test 6 (persistence) requires manual verification because automated reboots in QEMU are complex.

3. **Single interface only**: Currently only tests eth0. Multi-interface scenarios (VLAN, bonding) are not tested.

4. **No DHCP client testing**: The test verifies DHCP fallback behavior but doesn't test actual DHCP client functionality.

## Future Enhancements

- [ ] Add actual network connectivity tests (ping, DNS resolution)
- [ ] Test VLAN configuration
- [ ] Test bonding configuration
- [ ] Test custom routes
- [ ] Automated reboot testing
- [ ] Multi-interface scenarios
- [ ] Network failure scenarios
- [ ] Configuration rollback testing

## Troubleshooting

### Test hangs
- Check if port 50052 is already in use
- Increase TIMEOUT value
- Check QEMU logs for boot failures

### osctl connection refused
- Verify keel-agent started successfully
- Check gRPC server is listening on port 50051
- Verify port forwarding in QEMU (50052→50051)

### Configuration not saved
- Check `/var/lib/keel/network/config.json` in QEMU
- Verify disk image is writable
- Check keel-agent logs for errors

## Related Documentation

- [Network Management API](../../docs/reference/network-api.md)
- [Networking Guide](../../docs/learn-more/networking.md)
- [Testing Guide](./README.md)
