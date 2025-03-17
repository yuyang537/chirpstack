@echo off
REM 运行KLEE符号执行分析的Windows批处理脚本

REM 检查KLEE是否已安装
where klee >nul 2>nul
if %ERRORLEVEL% neq 0 (
    echo 错误: KLEE未安装。请先安装KLEE。
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

REM 编译ChirpStack为LLVM位码
echo 编译ChirpStack为LLVM位码...
set RUSTFLAGS=-Ccodegen-units=1 -Clink-arg=-Wl,--export-dynamic
cargo rustc --bin chirpstack --features klee --release -- --emit=llvm-bc

REM 查找生成的位码文件
for /f "tokens=*" %%a in ('dir /b /s target\release\deps\chirpstack-*.bc') do (
    set BITCODE_FILE=%%a
    goto :found_bitcode
)

echo 错误: 未找到位码文件。编译可能失败。
exit /b 1

:found_bitcode
echo 找到位码文件: %BITCODE_FILE%

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