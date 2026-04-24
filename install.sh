#!/bin/bash
set -e

BINARY_NAME="hremap"
BINARY_PATH="./target/release/$BINARY_NAME"
INSTALL_PATH="/usr/local/bin/$BINARY_NAME"
CONFIG_DIR="$HOME/.config/$BINARY_NAME"
CONFIG_FILE="config.yaml"
SERVICE_DIR="$HOME/.config/systemd/user"
SERVICE_FILE="$SERVICE_DIR/$BINARY_NAME.service"

# ─── Colors ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # no color

info()    { echo -e "${GREEN}[INFO]${NC} $1"; }
warn()    { echo -e "${YELLOW}[WARN]${NC} $1"; }
error()   { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# ─── Parse arguments ─────────────────────────────────────────────────────────
REPLACE_CONFIG=false

for arg in "$@"; do
    case $arg in
        --replace-config)
            REPLACE_CONFIG=true
            ;;
        --help|-h)
            echo "Usage: ./install.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --replace-config   Replace existing config with config.yaml from project root"
            echo "  --help             Show this help message"
            exit 0
            ;;
        *)
            error "Unknown argument: $arg. Use --help for usage."
            ;;
    esac
done

# ─── Check we're in the right directory ──────────────────────────────────────
if [ ! -f "Cargo.toml" ]; then
    error "Please run this script from the hremap project root directory"
fi

# ─── Build release binary ────────────────────────────────────────────────────
info "Building release binary..."
cargo build --release
if [ ! -f "$BINARY_PATH" ]; then
    error "Build failed — binary not found at $BINARY_PATH"
fi
info "Build successful"

# ─── Install binary ──────────────────────────────────────────────────────────
info "Installing binary to $INSTALL_PATH..."
sudo cp "$BINARY_PATH" "$INSTALL_PATH"
sudo chmod +x "$INSTALL_PATH"
info "Binary installed"

# ─── Set up config directory ─────────────────────────────────────────────────
mkdir -p "$CONFIG_DIR"

copy_config() {
    if [ -f "$CONFIG_FILE" ]; then
        info "Copying config.yaml to $CONFIG_DIR/$CONFIG_FILE..."
        cp "$CONFIG_FILE" "$CONFIG_DIR/$CONFIG_FILE"
        info "Config installed"
    elif [ -f "config.yaml.example" ]; then
        info "config.yaml not found, copying config.yaml.example to $CONFIG_DIR/$CONFIG_FILE..."
        cp "config.yaml.example" "$CONFIG_DIR/$CONFIG_FILE"
        warn "Config installed from example — please edit $CONFIG_DIR/$CONFIG_FILE before use"
    else
        warn "No config.yaml or config.yaml.example found — skipping config copy"
        warn "Please manually create $CONFIG_DIR/$CONFIG_FILE"
    fi
}

if [ "$REPLACE_CONFIG" = true ]; then
    info "Replacing config (--replace-config flag set)..."
    copy_config
elif [ ! -f "$CONFIG_DIR/$CONFIG_FILE" ]; then
    copy_config
else
    info "Config already exists at $CONFIG_DIR/$CONFIG_FILE — skipping (use --replace-config to overwrite)"
fi

# ─── Add user to input group if not already ──────────────────────────────────
if ! groups "$USER" | grep -q "\binput\b"; then
    info "Adding $USER to input group..."
    sudo usermod -aG input "$USER"
    warn "You need to log out and back in for the input group to take effect"
else
    info "User $USER is already in the input group"
fi

# ─── Create systemd user service ─────────────────────────────────────────────
info "Creating systemd user service..."
mkdir -p "$SERVICE_DIR"

cat > "$SERVICE_FILE" << EOF
[Unit]
Description=hremap - key remapper
After=graphical-session.target
Wants=graphical-session.target

[Service]
Type=simple
ExecStart=$INSTALL_PATH $CONFIG_DIR/$CONFIG_FILE
Restart=on-failure
RestartSec=3

[Install]
WantedBy=graphical-session.target
EOF

info "Service file created at $SERVICE_FILE"

# ─── Enable and start service ────────────────────────────────────────────────
info "Enabling and starting hremap service..."
systemctl --user daemon-reload
systemctl --user enable "$BINARY_NAME.service"
systemctl --user restart "$BINARY_NAME.service"

# ─── Verify ──────────────────────────────────────────────────────────────────
sleep 1
if systemctl --user is-active --quiet "$BINARY_NAME.service"; then
    info "hremap is running successfully!"
else
    error "hremap failed to start — check logs with: journalctl --user -u $BINARY_NAME.service -f"
fi

# ─── Done ────────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}Installation complete!${NC}"
echo ""
echo "Useful commands:"
echo "  journalctl --user -u hremap.service -f   # view logs"
echo "  systemctl --user restart hremap.service  # restart after config change"
echo "  systemctl --user stop hremap.service     # stop"
echo "  systemctl --user status hremap.service   # check status"
echo ""
echo "Config file: $CONFIG_DIR/$CONFIG_FILE"