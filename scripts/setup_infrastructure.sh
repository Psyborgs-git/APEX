#!/bin/bash

# Infrastructure setup script for APEX Terminal
# Deploys TimescaleDB, Redis, Prometheus, and Grafana

set -e

echo "🚀 Setting up APEX infrastructure..."

# Create monitoring directories
mkdir -p monitoring/grafana/dashboards
mkdir -p monitoring/grafana/datasources

# Create Grafana datasource config
cat > monitoring/grafana/datasources/prometheus.yml <<EOF
apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
    isDefault: true
    editable: true
EOF

# Create Grafana dashboard provisioning config
cat > monitoring/grafana/dashboards/dashboard.yml <<EOF
apiVersion: 1

providers:
  - name: 'APEX Dashboards'
    orgId: 1
    folder: ''
    type: file
    disableDeletion: false
    updateIntervalSeconds: 10
    allowUiUpdates: true
    options:
      path: /etc/grafana/provisioning/dashboards
EOF

# Start infrastructure
echo "📦 Starting Docker containers..."
docker-compose -f docker-compose.infrastructure.yml up -d

# Wait for services to be healthy
echo "⏳ Waiting for services to be healthy..."
sleep 10

# Check TimescaleDB
echo "🔍 Checking TimescaleDB..."
until docker exec apex-timescaledb pg_isready -U apex > /dev/null 2>&1; do
  echo "Waiting for TimescaleDB..."
  sleep 2
done
echo "✅ TimescaleDB is ready"

# Check Redis
echo "🔍 Checking Redis..."
until docker exec apex-redis redis-cli ping > /dev/null 2>&1; do
  echo "Waiting for Redis..."
  sleep 2
done
echo "✅ Redis is ready"

echo ""
echo "🎉 Infrastructure setup complete!"
echo ""
echo "Services:"
echo "  - TimescaleDB: localhost:5432 (user: apex, db: apex_market)"
echo "  - Redis: localhost:6379"
echo "  - Prometheus: http://localhost:9090"
echo "  - Grafana: http://localhost:3001 (admin/admin)"
echo ""
echo "To stop infrastructure: docker-compose -f docker-compose.infrastructure.yml down"
echo "To view logs: docker-compose -f docker-compose.infrastructure.yml logs -f"
