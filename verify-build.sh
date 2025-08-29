#!/bin/bash

# Simple build verification script for the Holographic C2 project

set -e

echo "🚀 Starting Holographic C2 build verification..."

# Check for required dependencies
echo "📋 Checking prerequisites..."

if ! command -v cargo &> /dev/null; then
    echo "❌ Cargo (Rust) is required but not installed."
    exit 1
fi

if ! command -v protoc &> /dev/null; then
    echo "⚠️  Protocol Buffer compiler (protoc) not found. Install with:"
    echo "   macOS: brew install protobuf"
    echo "   Ubuntu: sudo apt-get install protobuf-compiler"
    echo ""
fi

echo "✅ Prerequisites check completed"

# Format check
echo "🔍 Checking code formatting..."
if ! cargo fmt --all -- --check; then
    echo "❌ Code formatting issues detected. Run 'cargo fmt --all' to fix."
    exit 1
fi
echo "✅ Code formatting is correct"

# Quick compile check (no full build due to time)
echo "🏗️  Running quick compile check..."
cargo check --workspace --locked
echo "✅ Compile check passed"

echo ""
echo "🎉 Holographic C2 project verification completed successfully!"
echo ""
echo "To run the full system:"
echo "  1. Build workspace: cargo build --workspace"
echo "  2. Start infrastructure: docker-compose up --build"
echo "  3. Run agent: ORCHESTRATOR_PUBLIC_GRPC_ADDR=http://127.0.0.1:60051 cargo run --bin sim_agent"
echo "  4. Run viewer: cargo run --bin holographic-viewer -- --c2-grpc-addr http://127.0.0.1:60051"
echo "  5. Access Grafana: http://localhost:3000 (admin/admin)"
echo ""
