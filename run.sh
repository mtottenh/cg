#!/bin/bash
set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Gaming Portal API - Development Server${NC}"
echo ""

# Check for .env file
if [ ! -f .env ]; then
    echo -e "${YELLOW}No .env file found. Creating from .env.example...${NC}"
    cp .env.example .env
fi

# Load environment variables
set -a
source .env
set +a

# Check if PostgreSQL is running
if ! docker compose ps postgres 2>/dev/null | grep -q "running"; then
    echo -e "${YELLOW}Starting PostgreSQL...${NC}"
    docker compose up -d postgres

    # Wait for PostgreSQL to be ready
    echo "Waiting for PostgreSQL to be ready..."
    until docker compose exec -T postgres pg_isready -U portal -d portal_dev > /dev/null 2>&1; do
        sleep 1
    done
    echo -e "${GREEN}PostgreSQL is ready!${NC}"
fi

PORT=3000
echo ""
echo -e "${GREEN}Starting API server...${NC}"
echo "  - API:        http://localhost:${PORT:-3000}"
echo "  - Swagger UI: http://localhost:${PORT:-3000}/swagger-ui"
echo "  - Health:     http://localhost:${PORT:-3000}/health"
echo ""

# Run the server (migrations run automatically on startup)
cargo run -p portal-app
