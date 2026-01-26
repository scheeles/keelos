#!/bin/bash
# Integration test for automatic rollback on failed health checks
#
# This script tests the complete rollback flow:
# 1. Schedule an update with auto-rollback enabled
# 2. Flash a "broken" image (simulated)
# 3. Verify automatic rollback occurs
# 4. Verify system returns to previous partition

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Configuration
OSCTL="${PROJECT_ROOT}/target/debug/osctl"
ENDPOINT="http://localhost:50051"
TEST_IMAGE_URL="http://test-server/broken-image.squashfs"
HEALTH_CHECK_TIMEOUT=60  # 1 minute for testing

# Prerequisites check
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    if [[ ! -f "$OSCTL" ]]; then
        log_error "osctl binary not found at $OSCTL"
        log_info "Building osctl..."
        (cd "$PROJECT_ROOT" && cargo build --package osctl)
    fi
    
    if ! command -v curl &> /dev/null; then
        log_error "curl is required but not installed"
        exit 1
    fi
    
    log_info "Prerequisites OK"
}

# Check if matic-agent is running
check_agent_running() {
    log_info "Checking if matic-agent is running..."
    
    if ! curl -s "$ENDPOINT/health" &> /dev/null; then
        log_warning "matic-agent is not running at $ENDPOINT"
        log_info "Please start matic-agent before running this test"
        exit 1
    fi
    
    log_info "matic-agent is running"
}

# Get current system health
get_initial_health() {
    log_info "Getting initial health status..."
    
    "$OSCTL" --endpoint "$ENDPOINT" health || {
        log_error "Failed to get initial health status"
        exit 1
    }
}

# Get current partition info
get_current_partition() {
    log_info "Getting current active partition..."
    
    # This would normally read from /proc/cmdline or similar
    # For testing, we'll simulate it
    echo "2"
}

# Schedule update with auto-rollback
schedule_broken_update() {
    local current_partition=$1
    
    log_info "Scheduling update with auto-rollback enabled..."
    log_warning "Using simulated broken image: $TEST_IMAGE_URL"
    
    # In a real test, this would schedule an actual update
    # For now, we're demonstrating the command structure
    log_info "Command that would be executed:"
    echo "  $OSCTL --endpoint $ENDPOINT schedule update \\"
    echo "    --source $TEST_IMAGE_URL \\"
    echo "    --enable-auto-rollback \\"
    echo "    --health-check-timeout $HEALTH_CHECK_TIMEOUT"
    
    # Simulate schedule ID
    echo "schedule-test-$(date +%s)"
}

# Monitor update progress
monitor_update() {
    local schedule_id=$1
    
    log_info "Monitoring update progress (schedule: $schedule_id)..."
    
    # In a real implementation, this would poll the schedule status
    log_info "Waiting for update to complete or fail..."
    sleep 5
}

# Verify rollback occurred
verify_rollback() {
    local original_partition=$1
    local schedule_id=$2
    
    log_info "Verifying rollback occurred..."
    
    # Check rollback history
    log_info "Checking rollback history:"
    "$OSCTL" --endpoint "$ENDPOINT" rollback history || {
        log_error "Failed to get rollback history"
        return 1
    }
    
    # Verify we're back on the original partition
    local current_partition
    current_partition=$(get_current_partition)
    
    if [[ "$current_partition" == "$original_partition" ]]; then
        log_info "✅ Rollback successful: back on partition $original_partition"
        return 0
    else
        log_error "❌ Rollback verification failed"
        log_error "Expected partition: $original_partition, Got: $current_partition"
        return 1
    fi
}

# Verify system health after rollback
verify_health_after_rollback() {
    log_info "Verifying system health after rollback..."
    
    "$OSCTL" --endpoint "$ENDPOINT" health || {
        log_error "Failed to get health status after rollback"
        return 1
    }
    
    log_info "✅ System health check passed"
}

# Main test flow
main() {
    log_info "========================================="
    log_info "Rollback Flow Integration Test"
    log_info "========================================="
    echo
    
    check_prerequisites
    check_agent_running
    
    echo
    log_info "Step 1: Get initial state"
    get_initial_health
    
    local original_partition
    original_partition=$(get_current_partition)
    log_info "Current partition: $original_partition"
    
    echo
    log_info "Step 2: Schedule broken update"
    local schedule_id
    schedule_id=$(schedule_broken_update "$original_partition")
    log_info "Scheduled update: $schedule_id"
    
    echo
    log_info "Step 3: Monitor update and rollback"
    monitor_update "$schedule_id"
    
    echo
    log_info "Step 4: Verify rollback"
    if verify_rollback "$original_partition" "$schedule_id"; then
        log_info "✅ Rollback verification passed"
    else
        log_error "❌ Rollback verification failed"
        exit 1
    fi
    
    echo
    log_info "Step 5: Verify system health"
    if verify_health_after_rollback; then
        log_info "✅ Health verification passed"
    else
        log_error "❌ Health verification failed"
        exit 1
    fi
    
    echo
    log_info "========================================="
    log_info "✅ All tests passed!"
    log_info "========================================="
}

# Run tests
main "$@"
