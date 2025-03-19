#!/bin/bash
# 解决KLEE Include路径配置问题

# 颜色输出
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== 配置KLEE包含路径 ===${NC}"

# 1. 查找KLEE安装路径
echo -e "${GREEN}[1/3] 查找KLEE安装路径...${NC}"

KLEE_PATHS=(
  "/usr/local/include/klee"  # 默认安装位置
  "/usr/include/klee"        # 系统安装位置
  "$HOME/.local/include/klee" # 用户安装位置
  "$HOME/klee/include/klee"   # 从源码编译
)

KLEE_PATH=""
for path in "${KLEE_PATHS[@]}"; do
  if [ -d "$path" ]; then
    KLEE_PATH=$(dirname "$path")
    echo "找到KLEE头文件路径: $KLEE_PATH"
    break
  fi
done

if [ -z "$KLEE_PATH" ]; then
  echo "未找到KLEE头文件路径，请手动指定:"
  read -p "KLEE include路径: " KLEE_PATH
fi

# 2. 更新构建脚本
echo -e "${GREEN}[2/3] 更新构建脚本...${NC}"

# 更新C驱动构建脚本
sed -i "s|KLEE_INCLUDE_PATH=\"/usr/local/include\"|KLEE_INCLUDE_PATH=\"$KLEE_PATH\"|g" build_and_run.sh
sed -i "s|KLEE_INCLUDE_PATH=\"/usr/local/include\"|KLEE_INCLUDE_PATH=\"$KLEE_PATH\"|g" build_and_run_rust.sh

# 3. 创建编辑器配置文件
echo -e "${GREEN}[3/3] 创建编辑器配置文件...${NC}"

# VSCode C/C++配置
mkdir -p .vscode
cat > .vscode/c_cpp_properties.json <<EOF
{
    "configurations": [
        {
            "name": "KLEE Configuration",
            "includePath": [
                "\${workspaceFolder}/**",
                "$KLEE_PATH"
            ],
            "defines": [],
            "compilerPath": "/usr/bin/clang",
            "cStandard": "c11",
            "cppStandard": "c++14",
            "intelliSenseMode": "clang-x64"
        }
    ],
    "version": 4
}
EOF

# VisualStudio属性表
cat > klee_vs_props.props <<EOF
<?xml version="1.0" encoding="utf-8"?>
<Project ToolsVersion="4.0" xmlns="http://schemas.microsoft.com/developer/msbuild/2003">
  <ImportGroup Label="PropertySheets" />
  <PropertyGroup Label="UserMacros" />
  <PropertyGroup />
  <ItemDefinitionGroup>
    <ClCompile>
      <AdditionalIncludeDirectories>$KLEE_PATH;%(AdditionalIncludeDirectories)</AdditionalIncludeDirectories>
    </ClCompile>
  </ItemDefinitionGroup>
  <ItemGroup />
</Project>
EOF

echo -e "${BLUE}=== KLEE包含路径配置完成 ===${NC}"
echo "VSCode配置文件创建在: $(pwd)/.vscode/c_cpp_properties.json"
echo "Visual Studio属性表创建在: $(pwd)/klee_vs_props.props" 