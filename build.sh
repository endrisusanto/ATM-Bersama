#!/bin/bash

# ATM Auto Runner Local Build Script (DEB & RPM)
# ---------------------------------------------

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}⚡ Starting Local Build for ATM Auto Runner...${NC}"

# 1. Check for dependencies
echo -e "\n${BLUE}[1/4] Checking Dependencies...${NC}"

check_cmd() {
    if ! command -v $1 &> /dev/null; then
        echo -e "${RED}Error: $1 is not installed.${NC}"
        return 1
    fi
    echo -e "${GREEN}✓ $1 found.${NC}"
    return 0
}

check_cmd "npm" || exit 1
check_cmd "cargo" || exit 1
check_cmd "rpm" || echo -e "${RED}Warning: 'rpm' tool not found. RPM build will fail. (sudo apt install rpm)${NC}"

# 2. Install Frontend Dependencies
echo -e "\n${BLUE}[2/4] Installing Frontend Dependencies...${NC}"
npm install
if [ $? -ne 0 ]; then
    echo -e "${RED}Error: npm install failed.${NC}"
    exit 1
fi

# 3. Run Tauri Build
echo -e "\n${BLUE}[3/4] Building DEB and RPM packages...${NC}"
# We explicitly specify bundles to avoid building AppImage if not needed locally
npx tauri build --bundles deb,rpm

if [ $? -ne 0 ]; then
    echo -e "${RED}Error: Tauri build failed.${NC}"
    exit 1
fi

# 4. Summary
echo -e "\n${GREEN}✨ Build Completed Successfully!${NC}"
echo -e "${BLUE}---------------------------------------${NC}"
echo -e "Output locations:"
ls -1 src-tauri/target/release/bundle/deb/*.deb 2>/dev/null | xargs -I {} echo -e "${GREEN}DEB: {}${NC}"
ls -1 src-tauri/target/release/bundle/rpm/*.rpm 2>/dev/null | xargs -I {} echo -e "${GREEN}RPM: {}${NC}"
echo -e "${BLUE}---------------------------------------${NC}"
