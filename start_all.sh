#!/bin/bash

# Function to clean up background processes on exit
cleanup() {
    echo "Shutting down all services..."
    # Kill the whole process group
    kill 0
}

# Register the cleanup function for exit signals
trap cleanup SIGINT SIGTERM EXIT

echo "[1/3] Starting API Server..."
cargo run -p api &

echo "[2/3] Starting Minibot..."
cargo run -p minibot &

echo "[3/3] Starting Web Server (Vite)..."
cd web && bun run dev &

echo ""
echo "=================================================="
echo "Press Ctrl+C to gracefully stop all services."
echo ""

# Wait for all background jobs to finish (or until interrupted)
wait
