//! The `scriptUsage` and `stringUsage` console commands (cod_lnxded
//! 0x809d2d0 and 0x809a780): retail's output over vcod's own VM, whose
//! threads, objects and strings are not retail's variable and memory trees.
//! docs/research/cod11-server-handshake.md, "Console commands over rcon".

use vcod_gsc::ThreadUsage;

const RULE: &str = "********************************\n";

/// `MT_InitBits`' tree: 65536 eight-byte buckets, bucket 0 never handed out.
const BUCKETS: usize = 0xffff;

/// Threads grouped by their whole call stack, each group's count and summed
/// locals and endons over the innermost position and its callers, then
/// `(class, count, fields)` for every object class that has any.
pub fn script_usage(threads: &[ThreadUsage], classes: &[(&str, usize, usize)]) -> String {
    let mut threads = threads.to_vec();
    threads.sort();
    let mut out = String::from(RULE);
    for group in threads.chunk_by(|a, b| a.stack == b.stack) {
        let vars: usize = group.iter().map(|t| t.vars).sum();
        let endons: usize = group.iter().map(|t| t.endons).sum();
        out.push_str(&format!(
            "count: {}, var usage: {vars}, endon usage: {endons}\n",
            group.len()
        ));
        for (i, (file, line)) in group[0].stack.iter().enumerate() {
            if i > 0 {
                out.push_str("called from:\n");
            }
            out.push_str(&format!(
                "(file '{}.gsc', line {line})\n",
                file.replace('/', "\\")
            ));
        }
    }
    out.push_str(RULE);
    for (class, count, vars) in classes.iter().filter(|c| c.1 > 0) {
        out.push_str(&format!(
            "ent type '{class}'... count: {count}, var usage: {vars}\n"
        ));
    }
    out.push_str(RULE);
    out
}

/// Each string as `SL_GetString` would size it in the tree: a four-byte
/// header and the NUL, rounded up to a power of two of buckets
/// (`MT_GetSize` 0x809ae08). The free space is reported as one block per
/// set bit, the tree a compacted heap would leave.
pub fn string_usage(lengths: &[usize]) -> String {
    let used: usize = lengths
        .iter()
        .map(|l| (l + 5).div_ceil(8).next_power_of_two())
        .sum();
    let free = BUCKETS.saturating_sub(used);
    let mut out = String::from(RULE);
    for n in 0..=16 {
        let blocks = (free >> n) & 1;
        out.push_str(&format!(
            "{n} subtree has {blocks} * {} = {} free buckets\n",
            1 << n,
            blocks << n
        ));
    }
    out.push_str(RULE);
    out.push_str(RULE);
    out.push_str(&format!(
        "total memory alloc buckets: {used} ({} instances)\n",
        lengths.len()
    ));
    out.push_str(&format!("total memory free buckets: {free}\n"));
    out.push_str(RULE);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(stack: &[(&str, u32)], vars: usize, endons: usize) -> ThreadUsage {
        ThreadUsage {
            stack: stack.iter().map(|(f, l)| (f.to_string(), *l)).collect(),
            vars,
            endons,
        }
    }

    /// Retail's shape (captured on 1.1d): one block per distinct stack, the
    /// waiting position first and every caller after a `called from:`.
    #[test]
    fn script_usage_groups_threads_by_stack() {
        let a = [("maps/mp/gametypes/dm", 40), ("maps/mp/gametypes/dm", 12)];
        let text = script_usage(
            &[
                thread(&a, 3, 1),
                thread(&[("maps/mp/_load", 7)], 2, 0),
                thread(&a, 4, 1),
            ],
            &[("entity", 211, 431), ("hudelem", 1, 1), ("pathnode", 0, 0)],
        );
        assert_eq!(
            text,
            "********************************\n\
             count: 1, var usage: 2, endon usage: 0\n\
             (file 'maps\\mp\\_load.gsc', line 7)\n\
             count: 2, var usage: 7, endon usage: 2\n\
             (file 'maps\\mp\\gametypes\\dm.gsc', line 40)\n\
             called from:\n\
             (file 'maps\\mp\\gametypes\\dm.gsc', line 12)\n\
             ********************************\n\
             ent type 'entity'... count: 211, var usage: 431\n\
             ent type 'hudelem'... count: 1, var usage: 1\n\
             ********************************\n"
        );
    }

    #[test]
    fn string_usage_sizes_strings_in_power_of_two_buckets() {
        // 3 + 5 bytes is one bucket, 4 + 5 two, 12 + 5 three, rounded to four.
        let text = string_usage(&[3, 4, 12]);
        assert!(text.contains("total memory alloc buckets: 7 (3 instances)\n"));
        assert!(text.contains("total memory free buckets: 65528\n"));
        // 65528 = 0b1111_1111_1111_1000.
        assert!(text.contains("2 subtree has 0 * 4 = 0 free buckets\n"));
        assert!(text.contains("3 subtree has 1 * 8 = 8 free buckets\n"));
        assert!(text.contains("16 subtree has 0 * 65536 = 0 free buckets\n"));
        assert_eq!(text.lines().count(), 23);
    }
}
