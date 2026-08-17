#!/bin/bash
# ============================================
# Parrot Agent Database Reset Script
# ============================================
# 此脚本会清空数据库并重新插入种子数据
# 用于端到端测试前的环境准备
# ============================================

set -e  # 遇到错误立即退出

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "========================================"
echo "Parrot Agent Database Reset"
echo "========================================"

# 加载 .env 文件
if [ -f .env ]; then
    export $(cat .env | grep -v '^#' | xargs)
fi

# 检查 DATABASE_URL
if [ -z "$DATABASE_URL" ]; then
    echo -e "${RED}Error: DATABASE_URL not set${NC}"
    echo "Please set DATABASE_URL in .env or environment"
    exit 1
fi

# 解析数据库连接信息
DB_NAME=$(echo $DATABASE_URL | sed -n 's|.*://.*@.*/\(.*\)|\1|p')
echo "Database: $DB_NAME"
echo ""

# 执行 SQL 脚本
echo "Executing reset script..."
psql $DATABASE_URL -f scripts/reset_database.sql

# 检查执行结果
if [ $? -eq 0 ]; then
    echo ""
    echo -e "${GREEN}[OK] Database reset complete!${NC}"
    echo "========================================"
    
    # 验证数据
    echo ""
    echo "Verifying data..."
    
    COMPANY_COUNT=$(psql $DATABASE_URL -t -c "SELECT COUNT(*) FROM companies;")
    USER_COUNT=$(psql $DATABASE_URL -t -c "SELECT COUNT(*) FROM auth_users;")
    AGENT_COUNT=$(psql $DATABASE_URL -t -c "SELECT COUNT(*) FROM agents;")
    ISSUE_COUNT=$(psql $DATABASE_URL -t -c "SELECT COUNT(*) FROM issues;")
    
    echo "  Companies: $COMPANY_COUNT"
    echo "  Users: $USER_COUNT"
    echo "  Agents: $AGENT_COUNT"
    echo "  Issues: $ISSUE_COUNT"
    echo ""
    echo -e "${GREEN}✅ Database is ready for testing!${NC}"
else
    echo ""
    echo -e "${RED}[ERROR] Database reset failed!${NC}"
    exit 1
fi
