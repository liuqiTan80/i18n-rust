//! 转译产物坐标 → 母语源码坐标的列映射
//!
//! 转译是逐 token 的原地替换，不增删换行，因此**行号恒等**；但中英文 token
//! 长度不同（如 `让` → `let`、`可变` → `mut`），同一行内的列号会发生偏移，
//! 必须回放替换过程才能还原到母语源码的列。
//!
//! 本模块供命令行使用。**不可与 LSP 侧的列映射混用**：LSP 协议要求 UTF-16
//! 代码单元列（`lsp/src/translation_cache.rs`），而命令行输出面向终端，
//! 采用**字符数**列——与 rustc JSON 诊断的 `column_start` 口径一致。
//! 算法同源，但列口径不同，混用会在非 BMP 字符（emoji 等）上产生偏移。

use crate::cache::SourceMapEntry;
use rustc_lexer::{TokenKind, tokenize};
use std::collections::HashMap;

/// 行内列映射分段点：一次长度变化的替换发生后记录的 (英文列, 中文列)
///
/// 两个分段点之间英文与中文同步增长（未替换文本原样输出），
/// 故段内可直接用 `en_col - zh_col` 的恒定差值换算。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnPoint {
    /// 转译产物的行内列（0-based，字符数）
    pub en_col: u32,
    /// 母语源码的行内列（0-based，字符数）
    pub zh_col: u32,
}

/// 逐行列映射表：回放转译过程得到，供诊断列号回译使用
#[derive(Debug, Clone)]
pub struct ColumnMap {
    /// `per_line[i]` 为第 i 行（0-based）的分段点，按 `en_col` 升序且以行首 (0,0) 起始
    per_line: Vec<Vec<ColumnPoint>>,
}

impl ColumnMap {
    /// 回放转译过程，构建逐行的列映射
    ///
    /// `zh_source` 为母语源码，`pipeline_map` 为转译管线的全量编辑表
    /// （`TranspileOutput::pipeline_map`，条目按母语源偏移升序）。
    /// 未命中编辑表的 token 视为原样输出，长度不变。
    pub fn build(zh_source: &str, pipeline_map: &[SourceMapEntry]) -> Self {
        // 索引：母语源字节偏移 → 编辑条目（token 级替换，一个 token 至多一条）
        let by_offset: HashMap<usize, &SourceMapEntry> =
            pipeline_map.iter().map(|e| (e.source_offset, e)).collect();

        let mut per_line: Vec<Vec<ColumnPoint>> = vec![vec![ColumnPoint {
            en_col: 0,
            zh_col: 0,
        }]];
        let mut zh_col = 0u32;
        let mut en_col = 0u32;
        let mut offset = 0usize;

        for token in tokenize(zh_source) {
            let start = offset;
            let text = &zh_source[offset..][..token.len];
            offset += token.len;

            // 空白与注释不被替换，原样输出；但可能跨行（块注释/连续换行），
            // 需逐字符处理以在新行重置行内列并记录行首分段点
            if matches!(
                token.kind,
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment { .. }
            ) {
                for c in text.chars() {
                    if c == '\n' {
                        zh_col = 0;
                        en_col = 0;
                        per_line.push(vec![ColumnPoint {
                            en_col: 0,
                            zh_col: 0,
                        }]);
                    } else {
                        zh_col += 1;
                        en_col += 1;
                    }
                }
                continue;
            }

            let zh_len = text.chars().count() as u32;
            let en_len = by_offset
                .get(&start)
                .map(|e| e.replacement.chars().count() as u32)
                .unwrap_or(zh_len);

            zh_col += zh_len;
            en_col += en_len;

            // 仅长度变化时记录分段点：长度未变则该 token 前后列关系不变
            if en_len != zh_len
                && let Some(row) = per_line.last_mut()
            {
                row.push(ColumnPoint { en_col, zh_col });
            }
        }

        Self { per_line }
    }

    /// 把转译产物的 1-based (行, 列) 映射回母语源码的 1-based (行, 列)
    ///
    /// 转译不增删换行，行号恒等返回。
    /// 落在替换区间内的位置按差值近似到最近的中文 token 起点——
    /// 这与 LSP 侧一致，诊断只需指向正确的 token，无需精确到字符内部。
    ///
    /// 行号越界（映射表行数不足）时原样返回，不臆造坐标。
    pub fn map_position(&self, line: u32, column: u32) -> (u32, u32) {
        let Some(row) = self.per_line.get(line.saturating_sub(1) as usize) else {
            return (line, column);
        };
        let target = column.saturating_sub(1) as i32; // 1-based → 0-based

        // 取最后一个 en_col <= target 的分段点（行首点保证存在，索引安全）
        let mut point = ColumnPoint {
            en_col: 0,
            zh_col: 0,
        };
        for p in row {
            if p.en_col as i32 <= target {
                point = *p;
            } else {
                break;
            }
        }

        let diff = point.en_col as i32 - point.zh_col as i32;
        let zh_col = (target - diff).max(0) as u32;
        (line, zh_col + 1) // 回到 1-based
    }

    /// 映射表覆盖的行数（供测试与诊断用）
    pub fn line_count(&self) -> usize {
        self.per_line.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 中文 `    让 可变 数量 = 10;`
    /// 英文 `    let mut 数量 = 10;`
    ///
    /// 字节偏移：4 个前导空格后「让」占 3 字节（偏移 4），
    /// 再 1 空格后「可变」占 6 字节（偏移 8）。
    fn sample() -> (&'static str, Vec<SourceMapEntry>) {
        let zh = "    让 可变 数量 = 10;";
        let edits = vec![
            SourceMapEntry::new(4, 3, "让", "let"),
            SourceMapEntry::new(8, 6, "可变", "mut"),
        ];
        (zh, edits)
    }

    #[test]
    fn test_map_position_keyword_columns() {
        let (zh, edits) = sample();
        let cm = ColumnMap::build(zh, &edits);

        // 英文列 5（1-based）= `let` 的首字符 → 中文列 5 = 「让」
        assert_eq!(cm.map_position(1, 5), (1, 5));
        // 英文列 9 = `mut` 的首字符 → 中文列 7 = 「可」
        assert_eq!(cm.map_position(1, 9), (1, 7));
        // 英文列 13 = 「数」 → 中文列 10 = 「数」（未被替换，但受前面两个替换影响）
        assert_eq!(cm.map_position(1, 13), (1, 10));
        // 英文列 16 = `=` → 中文列 13 = `=`
        assert_eq!(cm.map_position(1, 16), (1, 13));
    }

    /// 前导空格未被替换，列号应恒等
    #[test]
    fn test_map_position_before_any_replacement() {
        let (zh, edits) = sample();
        let cm = ColumnMap::build(zh, &edits);
        assert_eq!(cm.map_position(1, 1), (1, 1));
        assert_eq!(cm.map_position(1, 4), (1, 4));
    }

    /// 空编辑表 → 恒等映射（转译未改动任何 token 时不应漂移）
    #[test]
    fn test_identity_without_edits() {
        let cm = ColumnMap::build("    让 数量 = 1;", &[]);
        for col in 1..=15 {
            assert_eq!(cm.map_position(1, col), (1, col));
        }
    }

    /// 多行：行号恒等，且每行的列差独立累积（不跨行污染）
    #[test]
    fn test_multiline_rows_are_independent() {
        let zh = "让 x = 1;\n让 可变 y = 2;";
        // 字节偏移：第 1 行「让 x = 1;」占 0..9（「让」3 字节 + 7 个 ASCII），
        // `\n` 在偏移 10，故第 2 行「让」在 11、「可变」在 15
        let edits = vec![
            SourceMapEntry::new(0, 3, "让", "let"),
            SourceMapEntry::new(11, 3, "让", "let"),
            SourceMapEntry::new(15, 6, "可变", "mut"),
        ];
        let cm = ColumnMap::build(zh, &edits);
        assert_eq!(cm.line_count(), 2);

        // 第 1 行：`let x = 1;` 列 5 = `x` → 中文列 3 = `x`
        assert_eq!(cm.map_position(1, 5), (1, 3));
        // 第 2 行：`let mut y = 2;` 列 9 = `y` → 中文列 6 = `y`
        assert_eq!(cm.map_position(2, 9), (2, 6));
    }

    /// 行号越界时原样返回，不臆造坐标
    #[test]
    fn test_out_of_range_line_passthrough() {
        let (zh, edits) = sample();
        let cm = ColumnMap::build(zh, &edits);
        assert_eq!(cm.map_position(99, 7), (99, 7));
    }

    /// 列号为 0（防御：上游给出非法坐标）不应下溢 panic
    #[test]
    fn test_zero_column_no_underflow() {
        let (zh, edits) = sample();
        let cm = ColumnMap::build(zh, &edits);
        assert_eq!(cm.map_position(1, 0), (1, 1));
    }
}
