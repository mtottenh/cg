#!/bin/bash
set -e

# Wait for PostgreSQL to be ready
echo "Waiting for PostgreSQL to be ready..."
until PGPASSWORD="${POSTGRES_PASSWORD}" psql -h "${POSTGRES_HOST}" -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" -c '\q' 2>/dev/null; do
  echo "PostgreSQL is unavailable - sleeping..."
  sleep 2
done
echo "PostgreSQL is ready!"

# Bootstrap E2E admin if credentials are provided
if [ -n "${E2E_ADMIN_EMAIL}" ] && [ -n "${E2E_ADMIN_PASSWORD}" ]; then
  echo "Bootstrapping E2E admin user..."
  portal-cli bootstrap admin \
    --username "${E2E_ADMIN_USERNAME:-e2e_admin}" \
    --email "${E2E_ADMIN_EMAIL}" \
    --password "${E2E_ADMIN_PASSWORD}" \
    --display-name "${E2E_ADMIN_DISPLAY_NAME:-E2E Admin}" \
    --force 2>&1 || echo "Admin bootstrap completed (may already exist)"
fi

# Execute the main command
exec "$@"
