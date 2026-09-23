#!/usr/bin/env bash
# ==============================================================================
# Xboard-RS Linux One-Key Installer & Service Manager
# ==============================================================================
set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
PLAIN='\033[0m'

INSTALL_DIR="/opt/xboard"
SERVICE_NAME="xboard"
DEFAULT_REPO="vineprkl/xboardrs"

# ==============================================================================
# 0. Handle Uninstallation
# ==============================================================================
if [ "$1" = "uninstall" ] || [ "$1" = "--uninstall" ]; then
    echo -e "${RED}====================================================${PLAIN}"
    echo -e "${RED}           ⚠️  Xboard-RS 服务卸载程序              ${PLAIN}"
    echo -e "${RED}====================================================${PLAIN}"
    if [ "$EUID" -ne 0 ]; then
        echo -e "${RED}错误：请使用 root 用户或 sudo 执行卸载！${PLAIN}"
        exit 1
    fi

    # Stop and disable systemd service
    if systemctl is-active --quiet ${SERVICE_NAME} 2>/dev/null; then
        echo -e "${YELLOW}正在停止 ${SERVICE_NAME} 守护进程...${PLAIN}"
        systemctl stop ${SERVICE_NAME} 2>/dev/null || true
    fi
    if systemctl is-enabled --quiet ${SERVICE_NAME} 2>/dev/null; then
        echo -e "${YELLOW}正在禁用 ${SERVICE_NAME} 开机自启...${PLAIN}"
        systemctl disable ${SERVICE_NAME} 2>/dev/null || true
    fi

    # Remove systemd service
    if [ -f "/etc/systemd/system/${SERVICE_NAME}.service" ]; then
        rm -f "/etc/systemd/system/${SERVICE_NAME}.service"
        systemctl daemon-reload
        echo -e "${GREEN}已移除 Systemd 守护进程配置${PLAIN}"
    fi

    # Remove CLI symlink
    if [ -L "/usr/local/bin/xboard-rs" ] || [ -f "/usr/local/bin/xboard-rs" ]; then
        rm -f /usr/local/bin/xboard-rs
        echo -e "${GREEN}已移除 /usr/local/bin/xboard-rs 快捷命令${PLAIN}"
    fi

    # Handle installation files & data
    if [ -d "${INSTALL_DIR}" ]; then
        echo -e "${BLUE}----------------------------------------------------${PLAIN}"
        if [ -t 0 ]; then
            read -p "是否彻底删除数据库与数据目录 (${INSTALL_DIR}/data)？[y/N]: " REMOVE_DATA
            case "$REMOVE_DATA" in
                [yY][eE][sS]|[yY])
                    rm -rf "${INSTALL_DIR}"
                    echo -e "${GREEN}已彻底删除安装目录与全部数据: ${INSTALL_DIR}${PLAIN}"
                    ;;
                *)
                    rm -rf "${INSTALL_DIR}/xboard-rs" "${INSTALL_DIR}/public" "${INSTALL_DIR}/theme"
                    echo -e "${GREEN}已清除核心程序与静态资源，已保留数据: ${INSTALL_DIR}/data${PLAIN}"
                    ;;
            esac
        else
            rm -rf "${INSTALL_DIR}/xboard-rs" "${INSTALL_DIR}/public" "${INSTALL_DIR}/theme"
            echo -e "${GREEN}非交互模式：已清除核心程序，已安全保留数据: ${INSTALL_DIR}/data${PLAIN}"
        fi
    fi

    echo -e "${GREEN}====================================================${PLAIN}"
    echo -e "${GREEN}          🎉 Xboard-RS 已成功卸载完毕！             ${PLAIN}"
    echo -e "${GREEN}====================================================${PLAIN}"
    exit 0
fi

echo -e "${BLUE}====================================================${PLAIN}"
echo -e "${GREEN}        ⚡ Xboard-RS Linux 服务安装脚本          ${PLAIN}"
echo -e "${BLUE}====================================================${PLAIN}"

# Check root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}错误：请使用 root 用户或 sudo 执行此脚本！${PLAIN}"
    exit 1
fi

# Detect Architecture
ARCH=$(uname -m)
case "$ARCH" in
    x86_64|amd64)
        TARGET_ARCH="linux-amd64"
        ;;
    aarch64|arm64)
        TARGET_ARCH="linux-arm64"
        ;;
    *)
        echo -e "${RED}错误：当前系统架构 (${ARCH}) 暂不受支持！${PLAIN}"
        exit 1
        ;;
esac

echo -e "${GREEN}[1/5] 检测到系统架构: ${TARGET_ARCH}${PLAIN}"

# Create directories
echo -e "${GREEN}[2/5] 初始化安装目录: ${INSTALL_DIR}${PLAIN}"
mkdir -p "${INSTALL_DIR}/data"
mkdir -p "${INSTALL_DIR}/public"
mkdir -p "${INSTALL_DIR}/theme"

# Check local binary or download from GitHub Releases
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -f "${SCRIPT_DIR}/xboard-rs" ]; then
    echo -e "${BLUE}发现本地二进制程序，正在部署...${PLAIN}"
    cp -f "${SCRIPT_DIR}/xboard-rs" "${INSTALL_DIR}/xboard-rs"
    chmod +x "${INSTALL_DIR}/xboard-rs"
fi

if [ -d "${SCRIPT_DIR}/public" ]; then
    cp -rf "${SCRIPT_DIR}/public/"* "${INSTALL_DIR}/public/" 2>/dev/null || true
fi

if [ -d "${SCRIPT_DIR}/theme" ]; then
    cp -rf "${SCRIPT_DIR}/theme/"* "${INSTALL_DIR}/theme/" 2>/dev/null || true
fi

# Fallback: Download from GitHub Releases if run via one-line curl command
if [ ! -f "${INSTALL_DIR}/xboard-rs" ]; then
    REPO="${GITHUB_REPO:-$DEFAULT_REPO}"
    if [ -n "$REPO" ]; then
        echo -e "${BLUE}正在从 GitHub (${REPO}) 获取最新版本信息...${PLAIN}"
        LATEST_TAG=$(curl -sI "https://github.com/${REPO}/releases/latest" 2>/dev/null | grep -i '^location:' | sed -E 's/.*tag\/(.*)/\1/' | tr -d '\r\n')
        if [ -z "$LATEST_TAG" ]; then
            LATEST_TAG=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name":' | head -n 1 | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')
        fi

        TMP_FILE="/tmp/xboard-bundle.tar.gz"
        DOWNLOAD_SUCCESS=0

        # 1. 优先尝试按版本 Tag 下载
        if [ -n "$LATEST_TAG" ]; then
            echo -e "${BLUE}检测到最新版本: ${LATEST_TAG}，正在下载 ${TARGET_ARCH} Bundle 整合包...${PLAIN}"
            DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/xboard-rs-${LATEST_TAG}-${TARGET_ARCH}-bundle.tar.gz"
            if curl -fsSL -o "$TMP_FILE" "$DOWNLOAD_URL"; then
                DOWNLOAD_SUCCESS=1
            fi
        fi

        # 2. 回退尝试 latest 路径下载
        if [ "$DOWNLOAD_SUCCESS" -ne 1 ]; then
            echo -e "${BLUE}尝试从 latest 路径下载 ${TARGET_ARCH} Bundle 整合包...${PLAIN}"
            DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/xboard-rs-latest-${TARGET_ARCH}-bundle.tar.gz"
            if curl -fsSL -o "$TMP_FILE" "$DOWNLOAD_URL"; then
                DOWNLOAD_SUCCESS=1
            fi
        fi

        if [ "$DOWNLOAD_SUCCESS" -eq 1 ]; then
            echo -e "${GREEN}下载成功，正在解压部署...${PLAIN}"
            mkdir -p /tmp/xboard-unpack
            tar -zxvf "$TMP_FILE" -C /tmp/xboard-unpack/
            UNPACK_DIR=$(find /tmp/xboard-unpack -mindepth 1 -maxdepth 1 -type d | head -n 1)
            [ -z "$UNPACK_DIR" ] && UNPACK_DIR="/tmp/xboard-unpack"
            cp -f "${UNPACK_DIR}/xboard-rs" "${INSTALL_DIR}/xboard-rs"
            chmod +x "${INSTALL_DIR}/xboard-rs"
            [ -d "${UNPACK_DIR}/public" ] && cp -rf "${UNPACK_DIR}/public/"* "${INSTALL_DIR}/public/" 2>/dev/null || true
            [ -d "${UNPACK_DIR}/theme" ] && cp -rf "${UNPACK_DIR}/theme/"* "${INSTALL_DIR}/theme/" 2>/dev/null || true
            rm -rf /tmp/xboard-unpack "$TMP_FILE"
        else
            echo -e "${RED}自动下载失败，未能从 GitHub Releases 找到或下载 ${TARGET_ARCH} 安装包！${PLAIN}"
            echo -e "${YELLOW}请确认 GitHub Release 是否已发布相应架构的安装包，或手动将 xboard-rs 放置到 ${INSTALL_DIR}/xboard-rs 后重试。${PLAIN}"
            exit 1
        fi
    else
        echo -e "${RED}未配置 GitHub 仓库源且本地不存在可执行文件，安装终止！${PLAIN}"
        exit 1
    fi
fi

# 确保核心二进制必须存在
if [ ! -f "${INSTALL_DIR}/xboard-rs" ]; then
    echo -e "${RED}错误：未找到核心程序文件 (${INSTALL_DIR}/xboard-rs)，安装终止！${PLAIN}"
    exit 1
fi

# Initialize .env
echo -e "${GREEN}[3/5] 配置环境变量 (.env)...${PLAIN}"
if [ ! -f "${INSTALL_DIR}/.env" ]; then
    if [ -f "${SCRIPT_DIR}/.env.example" ]; then
        cp "${SCRIPT_DIR}/.env.example" "${INSTALL_DIR}/.env"
    else
        cat << 'EOF' > "${INSTALL_DIR}/.env"
SERVER_HOST=0.0.0.0
SERVER_PORT=7001
APP_NAME=Xboard
APP_URL=http://127.0.0.1:7001
DB_CONNECTION=sqlite
DB_DATABASE=/opt/xboard/data/xboard.db
RUST_LOG=xboard_rs=info,tower_http=info
EOF
    fi

    # Generate random 32-byte APP_KEY
    RANDOM_KEY=$(head -c 32 /dev/urandom | base64 | tr -dc 'a-zA-Z0-9' | head -c 32)
    sed -i "s|base64:xboard_default_key_32bytes!!|base64:${RANDOM_KEY}|g" "${INSTALL_DIR}/.env"
    echo -e "${GREEN}已自动生成全新安全 APP_KEY${PLAIN}"
else
    echo -e "${YELLOW}已存在 .env 配置，保留现有配置${PLAIN}"
fi

# Configure Systemd Service
echo -e "${GREEN}[4/5] 安装并配置 Systemd 守护进程...${PLAIN}"
cat << EOF > /etc/systemd/system/${SERVICE_NAME}.service
[Unit]
Description=Xboard-RS Enterprise High Performance Core
After=network.target network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
Group=root
WorkingDirectory=${INSTALL_DIR}
ExecStart=${INSTALL_DIR}/xboard-rs
Restart=always
RestartSec=3s
LimitNOFILE=65535
LimitNPROC=65535
Environment=RUST_LOG=xboard_rs=info,tower_http=info

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable ${SERVICE_NAME}

# Create convenient CLI symlink
ln -sf "${INSTALL_DIR}/xboard-rs" /usr/local/bin/xboard-rs 2>/dev/null || true

echo -e "${GREEN}[5/6] 启动 ${SERVICE_NAME} 服务并初始化数据库...${PLAIN}"
systemctl restart ${SERVICE_NAME}
sleep 2
if ! systemctl is-active --quiet ${SERVICE_NAME}; then
    echo -e "${RED}服务已创建，但启动状态异常，请通过 'journalctl -u ${SERVICE_NAME} -n 20' 查看日志。${PLAIN}"
    exit 1
fi

# Step 6: Interactive or Random Administrator Account Setup
echo -e "${GREEN}[6/6] 配置管理员账户...${PLAIN}"
if [ -t 0 ]; then
    # Interactive terminal
    echo -e "${BLUE}----------------------------------------------------${PLAIN}"
    read -p "请输入管理员邮箱 [默认: admin@xboard.local]: " INPUT_EMAIL
    ADMIN_EMAIL="${INPUT_EMAIL:-admin@xboard.local}"

    read -p "请输入管理员密码 [直接回车自动生成 16 位强随机密码]: " INPUT_PASSWORD
    if [ -z "$INPUT_PASSWORD" ]; then
        ADMIN_PASSWORD=$(head -c 32 /dev/urandom | base64 | tr -dc 'a-zA-Z0-9' | head -c 16)
        IS_RANDOM=1
    else
        ADMIN_PASSWORD="$INPUT_PASSWORD"
        IS_RANDOM=0
    fi
    echo -e "${BLUE}----------------------------------------------------${PLAIN}"
else
    # Non-interactive mode (e.g. pipe / automated script)
    ADMIN_EMAIL="admin@xboard.local"
    ADMIN_PASSWORD=$(head -c 32 /dev/urandom | base64 | tr -dc 'a-zA-Z0-9' | head -c 16)
    IS_RANDOM=1
fi

ADMIN_OUTPUT=$(cd "${INSTALL_DIR}" && ./xboard-rs admin "$ADMIN_EMAIL" "$ADMIN_PASSWORD")
ADMIN_PATH_URL=$(echo "$ADMIN_OUTPUT" | grep "Admin panel URL:" | awk '{print $NF}')
ADMIN_PATH=$(echo "$ADMIN_PATH_URL" | sed 's|.*:7001/||')

echo -e "${GREEN}====================================================${PLAIN}"
echo -e "${GREEN}   🎉 Xboard-RS 服务已成功安装并启动运行！         ${PLAIN}"
echo -e "${GREEN}====================================================${PLAIN}"
echo -e "服务端口:       ${BLUE}7001${PLAIN}"
if [ -n "$ADMIN_PATH" ]; then
    echo -e "后台管理地址:   ${GREEN}http://您的服务器IP:7001/${ADMIN_PATH}${PLAIN}"
fi
echo -e "管理员账号:     ${GREEN}${ADMIN_EMAIL}${PLAIN}"
if [ "$IS_RANDOM" -eq 1 ]; then
    echo -e "管理员密码:     ${YELLOW}${ADMIN_PASSWORD}${PLAIN} (已自动生成高强度随机密码)"
else
    echo -e "管理员密码:     ${YELLOW}${ADMIN_PASSWORD}${PLAIN}"
fi
echo -e "配置文件:       ${BLUE}${INSTALL_DIR}/.env${PLAIN}"
echo -e "数据目录:       ${BLUE}${INSTALL_DIR}/data/${PLAIN}"
echo -e "常用指令:"
echo -e "  查看运行状态: ${BLUE}systemctl status ${SERVICE_NAME}${PLAIN}"
echo -e "  查看实时日志: ${BLUE}journalctl -u ${SERVICE_NAME} -f${PLAIN}"
echo -e "  重置用户密码: ${BLUE}xboard-rs reset:password <邮箱> <新密码>${PLAIN}"
echo -e "  重设超管账号: ${BLUE}xboard-rs admin <邮箱> <密码>${PLAIN}"
echo -e "  一键卸载服务: ${BLUE}bash install.sh uninstall${PLAIN}"
echo -e "${GREEN}====================================================${PLAIN}"

