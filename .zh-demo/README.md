# zh-demo —— 方言能力演示项目

展示 rzc 方言改写能力的最小示例 crate，重点演示**模块路径替换**
（`使用 标准集合::哈希映射;` → `use std::collections::HashMap;`，
真实实现见 `crates/engine/src/模块路径.zh`）以及中英文标识符
混用时声明位与方法调用位的映射差异。

运行方式：在工作区根目录执行

```bash
rzc run .zh-demo/src/main.zh
```

注意：`src/main.zh` 中 `rustc_lexer` 保持英文原名——第三方库尚未
建立母语映射，属预期行为（可通过 `rzc mapping auto rustc_lexer`
生成映射后改善）。

自动校验：本演示的方言改写已纳入门禁——`make zh-demo-check`（脚本
`tools/zh-selfhost/demo-check.sh`）在隔离目录 eject `main.zh`，断言产物既出现预期
英文改写（`use std::collections::HashMap;`、`fn main()`、`println!`、第三方透传等），
又保留用户自定义中文类型（`struct 替换结果` 等），已接入 `make gate` 防演示随演化失效。
