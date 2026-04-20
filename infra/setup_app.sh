#!/bin/bash

set -e
set -o pipefail

# --- Configuration ---
GITHUB_ENV="production"

echo "Starting GitHub Secrets and Variables Initialization for '$GITHUB_ENV' environment..."
echo ""

# --- Prerequisite Checks ---

if ! command -v gh &> /dev/null; then
    echo "Error: gh CLI not found. Please install it and authenticate with 'gh auth login'."
    exit 1
fi

echo "Prerequisites met."

# --- Get Project Info ---
OWNER_REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)
if [ $? -ne 0 ]; then
    echo "Error: Could not determine GitHub repository." >&2
    exit 1
fi
echo "GitHub Repository: $OWNER_REPO"

# --- Read CONTEXT from GitHub vars (set by setup_rke2.sh) ---
CONTEXT=$(gh variable get CONTEXT --env $GITHUB_ENV --repo $OWNER_REPO 2>/dev/null || echo "")
if [ -z "$CONTEXT" ]; then
    echo "Error: CONTEXT not found. Run setup_rke2.sh first."
    exit 1
fi
echo "Context found: $CONTEXT"

# --- Docker Config (GitHub Container Registry) ---
if gh auth status 2>&1 | grep -q "write:packages"; then
    echo "gh CLI already has write:packages scope."
else
    echo "Adding write:packages scope to gh CLI..."
    gh auth refresh -s write:packages
    if [ $? -ne 0 ]; then
        echo "Error: Failed to refresh gh auth with write:packages scope."
        exit 1
    fi
fi

GH_TOKEN=$(gh auth token)
GH_USER=$(gh api user -q .login)
if [ -z "$GH_TOKEN" ] || [ -z "$GH_USER" ]; then
    echo "Error: Failed to get GitHub token or username."
    exit 1
fi

AUTH_BASE64=$(echo -n "$GH_USER:$GH_TOKEN" | base64 -w 0)
DOCKER_CONFIG_JSON=$(cat <<EOF
{
  "auths": {
    "ghcr.io": {
      "auth": "$AUTH_BASE64"
    }
  }
}
EOF
)
echo "Docker config for ghcr.io generated."

# --- Collect Information ---
echo ""
echo "--- Please provide the following information ---"

# --- Domain ---
read -p "Enter the domain for the application [late.sh]: " DOMAIN
DOMAIN=${DOMAIN:-late.sh}
GRAFANA_URL="grafana.$DOMAIN"

# --- Log Level ---
read -p "Enter the log level [info]: " LOG_LEVEL
LOG_LEVEL=${LOG_LEVEL:-info}

# --- SSH Host Key ---
echo ""
echo "--- SSH Host Key ---"
echo "Generating Ed25519 SSH host key for the SSH server..."
ssh-keygen -t ed25519 -f ssh_host_key -N "" -q
SSH_HOST_KEY=$(cat ssh_host_key)
echo "SSH host key generated."

# --- AI (optional) ---
echo ""
echo "--- AI Configuration (optional, press Enter to skip) ---"
read -p "Enter Gemini API key: " AI_API_KEY
read -p "Enter AI model [gemini-3.1-pro-preview]: " AI_MODEL
AI_MODEL=${AI_MODEL:-gemini-3.1-pro-preview}
if [ -n "$AI_API_KEY" ]; then
    AI_ENABLED="true"
else
    AI_ENABLED="false"
fi

# --- S3-Compatible Storage ---
TF_STATE_BUCKET="${CONTEXT}-tf-state"
DB_BACKUPS_BUCKET="${CONTEXT}-db-backups"

echo ""
echo "--- S3-Compatible Storage (for TF state and DB backups) ---"
echo "Create these two buckets in your provider before continuing:"
echo "  - $TF_STATE_BUCKET"
echo "  - $DB_BACKUPS_BUCKET"
echo ""
read -p "Enter S3 endpoint URL (e.g., https://s3.amazonaws.com): " S3_ENDPOINT
read -p "Enter S3 Access Key ID: " S3_ACCESS_KEY_ID
read -sp "Enter S3 Secret Access Key: " S3_SECRET_ACCESS_KEY
echo ""

# --- Confirmation ---
echo ""
echo "Please review the configuration below:"
echo "----------------------------------------"
echo "GitHub Repository:   $OWNER_REPO"
echo "GitHub Environment:  $GITHUB_ENV"
echo "Context:             $CONTEXT"
echo "Domain:              $DOMAIN"
echo "Grafana URL:         $GRAFANA_URL"
echo "Log Level:           $LOG_LEVEL"
echo "AI Enabled:          $AI_ENABLED"
if [ -n "$S3_ENDPOINT" ]; then
echo "S3 Endpoint:         $S3_ENDPOINT"
echo "TF State Bucket:     $TF_STATE_BUCKET"
echo "DB Backups Bucket:   $DB_BACKUPS_BUCKET"
fi
echo "----------------------------------------"
echo ""
read -p "Are you sure you want to continue? (yes/no): " confirmation
if [[ "$confirmation" != "yes" ]]; then
    echo "Cancelled."
    rm -f ssh_host_key ssh_host_key.pub
    exit 1
fi
echo ""

# --- Set GitHub Secrets ---
echo "Setting GitHub secrets..."
gh secret set DOCKER_CONFIG_JSON --body "$DOCKER_CONFIG_JSON" --env $GITHUB_ENV --repo $OWNER_REPO
gh secret set SSH_HOST_KEY --body "$SSH_HOST_KEY" --env $GITHUB_ENV --repo $OWNER_REPO
if [ -n "$S3_ACCESS_KEY_ID" ]; then
    gh secret set S3_ACCESS_KEY_ID --body "$S3_ACCESS_KEY_ID" --env $GITHUB_ENV --repo $OWNER_REPO
fi
if [ -n "$S3_SECRET_ACCESS_KEY" ]; then
    gh secret set S3_SECRET_ACCESS_KEY --body "$S3_SECRET_ACCESS_KEY" --env $GITHUB_ENV --repo $OWNER_REPO
fi
if [ -n "$AI_API_KEY" ]; then
    gh secret set AI_API_KEY --body "$AI_API_KEY" --env $GITHUB_ENV --repo $OWNER_REPO
fi
echo "Secrets set."

# --- Set GitHub Variables ---
echo "Setting GitHub variables..."
gh variable set LOG_LEVEL --body "$LOG_LEVEL" --env $GITHUB_ENV --repo $OWNER_REPO
gh variable set DOMAIN --body "$DOMAIN" --env $GITHUB_ENV --repo $OWNER_REPO
gh variable set GRAFANA_URL --body "$GRAFANA_URL" --env $GITHUB_ENV --repo $OWNER_REPO
gh variable set AI_MODEL --body "$AI_MODEL" --env $GITHUB_ENV --repo $OWNER_REPO
gh variable set AI_ENABLED --body "$AI_ENABLED" --env $GITHUB_ENV --repo $OWNER_REPO
if [ -n "$S3_ENDPOINT" ]; then
    gh variable set S3_ENDPOINT --body "$S3_ENDPOINT" --env $GITHUB_ENV --repo $OWNER_REPO
    gh variable set TF_STATE_BUCKET --body "$TF_STATE_BUCKET" --env $GITHUB_ENV --repo $OWNER_REPO
    gh variable set DB_BACKUPS_BUCKET --body "$DB_BACKUPS_BUCKET" --env $GITHUB_ENV --repo $OWNER_REPO
fi
echo "Variables set."

# --- Generate backend.tf ---
if [ -n "$S3_ENDPOINT" ]; then
    echo "Configuring Terraform backend..."
    TERRAFORM_BACKEND_TEMPLATE="backend.tf.tpl"
    TERRAFORM_BACKEND_FILE="backend.tf"
    if [ ! -f "$TERRAFORM_BACKEND_TEMPLATE" ]; then
        echo "Warning: $TERRAFORM_BACKEND_TEMPLATE not found. Skipping backend configuration."
    else
        cp "$TERRAFORM_BACKEND_TEMPLATE" "$TERRAFORM_BACKEND_FILE"
        sed -i "s/__TF_STATE_BUCKET__/$TF_STATE_BUCKET/" "$TERRAFORM_BACKEND_FILE"
        sed -i "s|__S3_ENDPOINT__|$S3_ENDPOINT|" "$TERRAFORM_BACKEND_FILE"
        echo "Terraform backend configured in $TERRAFORM_BACKEND_FILE"
    fi
fi

# --- Clean up ---
rm -f ssh_host_key ssh_host_key.pub
echo "Cleaned up temporary key files."

echo ""
echo "All secrets and variables have been set for the '$GITHUB_ENV' environment."
echo ""
echo "DNS Setup Required:"
echo "   Configure a wildcard DNS A record pointing to your server:"
echo "   *.$DOMAIN + $DOMAIN → <your-server-ip>"
echo ""
echo "   This enables:"
echo "   - ssh $DOMAIN (SSH TUI)"
echo "   - https://$DOMAIN (Web landing + pairing)"
echo "   - https://api.$DOMAIN (SSH API / WebSocket)"
echo "   - https://audio.$DOMAIN (Icecast audio stream)"
echo "   - https://grafana.$DOMAIN (Monitoring)"
echo ""
echo "Music files:"
echo "   After first deploy, copy music to the liquidsoap PVC:"
echo "   kubectl cp ./music/. \$(kubectl get pod -l app=liquidsoap -o name):/music/"
echo ""
