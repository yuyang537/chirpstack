@echo off
REM 运行KLEE符号执行分析的Windows批处理脚本

echo 在Windows环境中运行KLEE分析...

REM 检查KLEE是否已安装
where klee >nul 2>nul
if %ERRORLEVEL% neq 0 (
    echo 错误: KLEE未安装。请先安装KLEE。
    exit /b 1
)

REM 检查Rust是否已安装
where cargo >nul 2>nul
if %ERRORLEVEL% neq 0 (
    echo 错误: Rust未安装。请先安装Rust。
    echo 访问 https://rustup.rs 获取安装指南。
    exit /b 1
)

REM 检查LLVM工具链是否已安装
rustup component list --installed | findstr "llvm-tools" >nul
if %ERRORLEVEL% neq 0 (
    echo 安装LLVM工具链...
    rustup component add llvm-tools-preview
)

REM 进入ChirpStack目录
cd ..

REM 修改根目录的Cargo.toml，添加klee-chirpstack到工作区
echo 检查根目录的Cargo.toml是否包含klee-chirpstack...
findstr "klee-chirpstack" Cargo.toml >nul
if %ERRORLEVEL% neq 0 (
    echo 修改根目录的Cargo.toml，添加klee-chirpstack到工作区...
    copy Cargo.toml Cargo.toml.bak
    
    REM 在Windows中使用PowerShell进行文本替换
    powershell -Command "(Get-Content Cargo.toml) -replace '\[workspace\]', '[workspace]`n  resolver = \"2\"`n  members = [`n    \"chirpstack\",`n    \"chirpstack-integration\",`n    \"lrwn\",`n    \"lrwn-filters\",`n    \"backend\",`n    \"api/rust\",`n    \"klee-chirpstack\",`n  ]' | Set-Content Cargo.toml"
)

REM 修改chirpstack/Cargo.toml，使其引用本地的klee-sys包
echo 检查chirpstack/Cargo.toml是否引用本地的klee-sys包...
findstr "klee-sys.*path.*klee-chirpstack" chirpstack\Cargo.toml >nul
if %ERRORLEVEL% neq 0 (
    echo 修改chirpstack/Cargo.toml，使其引用本地的klee-sys包...
    copy chirpstack\Cargo.toml chirpstack\Cargo.toml.bak
    
    REM 在Windows中使用PowerShell进行文本替换
    powershell -Command "(Get-Content chirpstack\Cargo.toml) -replace 'klee-sys.*=.*\{.*version.*=.*\"0.1.0\".*\}', 'klee-sys = { path = \"../klee-chirpstack\" }' | Set-Content chirpstack\Cargo.toml"
    
    REM 确保klee特性正确配置
    powershell -Command "$content = Get-Content chirpstack\Cargo.toml -Raw; $pattern = '\[features\]([^\[]*?)klee.*=.*\[.*\]'; $replacement = '[features]$1klee = [\"klee-sys\", \"libc\", \"lrwn/crypto\"]'; $content = $content -replace $pattern, $replacement; Set-Content chirpstack\Cargo.toml -Value $content"
)

REM 进入实际的包目录
echo 进入chirpstack包目录...
cd chirpstack

REM 编译ChirpStack为LLVM位码
echo 编译ChirpStack为LLVM位码...
set RUSTFLAGS=-Ccodegen-units=1 -Clink-arg=-Wl,--export-dynamic
cargo rustc --bin chirpstack --features klee --release -- --emit=llvm-bc

REM 查找生成的位码文件
for /f "tokens=*" %%a in ('dir /b /s ..\target\release\deps\chirpstack-*.bc') do (
    set BITCODE_FILE=%%a
    goto :found_bitcode
)

echo 错误: 未找到位码文件。编译可能失败。
exit /b 1

:found_bitcode
echo 找到位码文件: %BITCODE_FILE%

REM 返回到根目录
cd ..

REM 创建配置目录
if not exist configuration mkdir configuration

REM 运行KLEE分析
echo 运行KLEE符号执行分析...
klee --libc=uclibc --posix-runtime --optimize --emit-all-errors --max-memory=4096 --max-time=3600 "%BITCODE_FILE%" --config ./configuration --command KleeSymbolicExecution

REM 分析结果
echo 分析KLEE结果...
klee-stats klee-out-*\ --print-all

echo KLEE分析完成。结果保存在klee-out-*目录中。
echo 您可以使用ktest-tool查看具体的测试用例，例如:
echo ktest-tool klee-out-*\test000001.ktest 