#!/bin/bash
# 逐个转译并编译验证代码块
DIR=$1
PASS=0; FAIL=0
for f in "$DIR"/block_*.zh; do
    name=$(basename "$f" .zh)
    if ! /home/t67/code/zrRust/target/debug/rzc eject "$f" -o /dev/null 2>/dev/null; then
        # eject 不支持 -o，用工作区方式
        /home/t67/code/zrRust/target/debug/rzc eject "$f" > /dev/null 2>&1
    fi
    rs="${f%.zh}.rs"
    if [ ! -f "$rs" ]; then
        # eject 输出到同名 .rs
        cp "$f" "$rs" 2>/dev/null
        /home/t67/code/zrRust/target/debug/rzc eject "$f" 2>/dev/null
    fi
    # 转译
    if [ -f "$rs" ]; then
        if rustc --edition 2021 -o "/tmp/${name}_bin" "$rs" 2>/tmp/${name}_err.txt; then
            # 有主函数就运行
            if grep -q "fn main" "$rs"; then
                if "/tmp/${name}_bin" > /tmp/${name}_out.txt 2>&1; then
                    echo "✅ $name 编译+运行成功: $(head -c 60 /tmp/${name}_out.txt | tr '\n' ' ')"
                else
                    echo "❌ $name 运行失败: $(head -c 120 /tmp/${name}_err.txt)"
                    FAIL=$((FAIL+1))
                fi
            else
                echo "✅ $name 编译成功（无主函数）"
            fi
            PASS=$((PASS+1))
        else
            echo "❌ $name 编译失败: $(head -c 200 /tmp/${name}_err.txt | tr '\n' ' ')"
            FAIL=$((FAIL+1))
        fi
    else
        echo "❌ $name 转译失败"
        FAIL=$((FAIL+1))
    fi
done
echo "======== 通过: $PASS, 失败: $FAIL ========"
