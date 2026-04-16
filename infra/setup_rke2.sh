#!/bin/bash

set -e
set -o pipefail

# --- Configuration ---
GITHUB_ENV="production"

echo "🚀 Starting RKE2 Kubernetes Cluster Setup for '$GITHUB_ENV' environment..."
echo ""

# --- Load Environment Variables ---
if [ ! -f .env ]; then
    echo "❌ .env file not found in infra/ directory."
    echo "Please create it from .env.example and configure your server details."
    exit 1
fi

set -a
source ./.env
set +a

# --- Prerequisite Checks ---
if ! command -v gh &> /dev/null; then
    echo "❌ gh CLI not found. Please install it and authenticate with 'gh auth login'."
    exit 1
fi

echo "✅ Prerequisites met."
echo ""
echo "--- Please provide the following information ---"

# --- Prompt for Context Name ---
read -p "Enter the context/app name (e.g., gofast-app-s): " CONTEXT
if [ -z "$CONTEXT" ]; then
    echo "❌ Context name cannot be empty."
    exit 1
fi

# --- Confirmation Step ---
echo ""
echo "Please review the configuration below:"
echo "----------------------------------------"
echo "Kubernetes Context: ${CONTEXT}"
echo "----------------------------------------"
echo "RKE2 Cluster Setup Configuration:"
echo "Server 1 IP:         ${SERVER_IP_1}"
echo "Server 1 User:       ${SERVER_USER_1}"
echo "Server 1 Name:       ${SERVER_NAME_1}"
if [ ! -z "${SERVER_IP_2}" ]; then
echo "Server 2 IP:         ${SERVER_IP_2}"
echo "Server 2 User:       ${SERVER_USER_2}"
echo "Server 2 Name:       ${SERVER_NAME_2}"
fi
if [ ! -z "${SERVER_IP_3}" ]; then
echo "Server 3 IP:         ${SERVER_IP_3}"
echo "Server 3 User:       ${SERVER_USER_3}"
echo "Server 3 Name:       ${SERVER_NAME_3}"
fi
if [ ! -z "${AGENT_IP}" ]; then
echo "Agent IP:            ${AGENT_IP}"
echo "Agent User:          ${AGENT_USER}"
echo "Agent Name:          ${AGENT_NAME}"
fi
echo "----------------------------------------"
echo "This script will perform the following actions:"
echo "  - Install RKE2 on the server nodes listed above."
if [ ! -z "${AGENT_IP}" ]; then
echo "  - Install RKE2 on the agent node listed above."
fi
echo "  - Configure your local kubeconfig to connect to the new cluster."
echo "  - Set the KUBE_CONFIG secret in the '$GITHUB_ENV' environment of your GitHub repository."
echo "  - Set the CONTEXT variable in the '$GITHUB_ENV' environment of your GitHub repository."
echo ""
read -p "Are you sure you want to continue? (yes/no): " confirmation
if [[ "$confirmation" != "yes" ]]; then
    echo "🛑 Initialization cancelled by user."
    exit 1
fi
echo ""


# ------ RKE2 HA Setup ------

echo "🔄 Installing RKE2 on the first server node..."
ssh -i ${SERVER_KEY_1} ${SERVER_USER_1}@${SERVER_IP_1} << EOF
    sudo mkdir -p /etc/rancher/rke2/
    echo "node-name: ${SERVER_NAME_1}" | sudo tee /etc/rancher/rke2/config.yaml
    curl -sfL https://get.rke2.io | sudo INSTALL_RKE2_TYPE="server" sh -
    sudo systemctl enable rke2-server.service
    sudo systemctl start rke2-server.service
EOF

echo "🔄 Extracting the server token..."
SERVER_TOKEN=$(ssh -i ${SERVER_KEY_1} ${SERVER_USER_1}@${SERVER_IP_1} 'sudo cat /var/lib/rancher/rke2/server/node-token')

if [ ! -z "${SERVER_IP_2}" ]; then
echo "🔄 Installing RKE2 on the second server node..."
ssh -i ${SERVER_KEY_2} ${SERVER_USER_2}@${SERVER_IP_2} << EOF
  curl -sfL https://get.rke2.io | sudo INSTALL_RKE2_TYPE="server" sh -
  sudo mkdir -p /etc/rancher/rke2/
  echo "server: https://${SERVER_IP_1}:9345" | sudo tee /etc/rancher/rke2/config.yaml
  echo "token: ${SERVER_TOKEN}" | sudo tee -a /etc/rancher/rke2/config.yaml
  echo "node-name: ${SERVER_NAME_2}" | sudo tee -a /etc/rancher/rke2/config.yaml
  sudo systemctl enable rke2-server.service
  sudo systemctl start rke2-server.service
EOF
else
  echo "SERVER_IP_2 not set. Skipping second server node installation."
fi

if [ ! -z "${SERVER_IP_3}" ]; then
echo "🔄 Installing RKE2 on the third server node..."
ssh -i ${SERVER_KEY_3} ${SERVER_USER_3}@${SERVER_IP_3} << EOF
  curl -sfL https://get.rke2.io | sudo INSTALL_RKE2_TYPE="server" sh -
  sudo mkdir -p /etc/rancher/rke2/
  echo "server: https://${SERVER_IP_1}:9345" | sudo tee /etc/rancher/rke2/config.yaml
  echo "token: ${SERVER_TOKEN}" | sudo tee -a /etc/rancher/rke2/config.yaml
  echo "node-name: ${SERVER_NAME_3}" | sudo tee -a /etc/rancher/rke2/config.yaml
  sudo systemctl enable rke2-server.service
  sudo systemctl start rke2-server.service
EOF
else
  echo "SERVER_IP_3 not set. Skipping third server node installation."
fi

if [ ! -z "${AGENT_IP}" ]; then
echo "🔄 Installing RKE2 on agent node..."
ssh -i ${AGENT_KEY} ${AGENT_USER}@${AGENT_IP} << EOF
  sudo mkdir -p /etc/rancher/rke2/
  sudo tee /etc/rancher/rke2/config.yaml > /dev/null <<CONFIG
server: https://${SERVER_IP_1}:9345
token: ${SERVER_TOKEN}
node-name: ${AGENT_NAME}
CONFIG
  curl -sfL https://get.rke2.io | sudo INSTALL_RKE2_TYPE="agent" sh -
  sudo systemctl enable rke2-agent.service
  sudo systemctl start rke2-agent.service
EOF
else
  echo "AGENT_IP not set. Skipping worker node installation."
fi

echo "🔄 Configuring local kubeconfig..."
mkdir -p $HOME/.kube
touch $HOME/.kube/config
rm -f $HOME/.kube/config_new
rm -f $HOME/.kube/config_backup
mv $HOME/.kube/config $HOME/.kube/config_backup

# Copy the kubeconfig from the first server node
scp -i ${SERVER_KEY_1} ${SERVER_USER_1}@${SERVER_IP_1}:/etc/rancher/rke2/rke2.yaml $HOME/.kube/config_new
sed -i "s/127.0.0.1/${SERVER_IP_1}/g" $HOME/.kube/config_new
sed -i "s/default/${CONTEXT}/g" $HOME/.kube/config_new

# Merge the kubeconfig files
KUBECONFIG=$HOME/.kube/config_backup:$HOME/.kube/config_new kubectl config view --merge --flatten > $HOME/.kube/config

# Switch to the specified context
kubectl config use-context $CONTEXT

echo "🔄 Verifying Kubernetes context..."
CURRENT_KUBECTL_CONTEXT=$(kubectl config current-context)
if [ "$CURRENT_KUBECTL_CONTEXT" != "$CONTEXT" ]; then
  echo "Error: Not on the correct Kubernetes context. Expected '$CONTEXT', but on '$CURRENT_KUBECTL_CONTEXT'." >&2
  echo "Please ensure your kubeconfig is correctly set up and the context '$CONTEXT' exists and is active." >&2
  exit 1
else
  echo "📦 Context '$CONTEXT' verified successfully."
fi

echo "🔄 Setting GitHub secrets and variables for '$GITHUB_ENV' environment..."

OWNER_REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)
if [ $? -ne 0 ]; then
    echo "❌ Error: Could not determine GitHub repository. Please ensure you are in a valid git repository with a GitHub remote." >&2
    exit 1
fi

echo "🔄 Ensuring '$GITHUB_ENV' environment exists in GitHub..."
gh api repos/$OWNER_REPO/environments/$GITHUB_ENV -X PUT > /dev/null

echo "🔄 Setting CONTEXT variable..."
gh variable set CONTEXT --body "$CONTEXT" --env $GITHUB_ENV --repo $OWNER_REPO
echo "✅ Variable 'CONTEXT' set for '$GITHUB_ENV' environment."

echo "🔄 Setting KUBE_CONFIG secret..."
kubectl config view --minify --raw | gh secret set KUBE_CONFIG --env $GITHUB_ENV --repo $OWNER_REPO
echo "✅ Secret 'KUBE_CONFIG' set for '$GITHUB_ENV' environment."

configure_host_ssh_port() {
  local server_ip="$1"
  local server_user="$2"
  local server_key="$3"
  local server_name="$4"

  if [ -z "$server_ip" ]; then
    return
  fi

  echo "🔄 Reconfiguring host SSH on ${server_name} (${server_ip}) to port 22222..."
  ssh -i "${server_key}" "${server_user}@${server_ip}" <<'EOF'
    set -e
    sudo mkdir -p /etc/ssh/sshd_config.d
    sudo tee /etc/ssh/sshd_config.d/99-admin.conf >/dev/null <<'CONFIG'
Port 22222
CONFIG
    sudo /usr/sbin/sshd -t
    sudo systemctl reload ssh
  EOF
}

echo "🔄 Moving host admin SSH to port 22222..."
configure_host_ssh_port "${SERVER_IP_1}" "${SERVER_USER_1}" "${SERVER_KEY_1}" "${SERVER_NAME_1}"
configure_host_ssh_port "${SERVER_IP_2}" "${SERVER_USER_2}" "${SERVER_KEY_2}" "${SERVER_NAME_2}"
configure_host_ssh_port "${SERVER_IP_3}" "${SERVER_USER_3}" "${SERVER_KEY_3}" "${SERVER_NAME_3}"
configure_host_ssh_port "${AGENT_IP}" "${AGENT_USER}" "${AGENT_KEY}" "${AGENT_NAME}"
echo "✅ Host admin SSH moved to port 22222 on configured nodes."

echo ""
echo "🎉 RKE2 setup completed successfully!"
echo "Run 'k9s' or 'kubectl get nodes' to verify your cluster is operational."
echo ""
