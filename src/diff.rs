pub const CONTEXT: usize = 3;

const MAX_DP_LINES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub added: usize,
    pub removed: usize,
}

impl Stats {
    pub fn label(&self) -> String {
        if self.is_empty() {
            return String::new();
        }
        format!("(+{} / -{})", self.added, self.removed)
    }

    pub fn is_empty(&self) -> bool {
        self.added == 0 && self.removed == 0
    }

    pub fn looks_like_rewrite(&self) -> bool {
        let max = self.added.max(self.removed);
        let min = self.added.min(self.removed);
        max >= 20 && min * 100 / max >= 60
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Added,
    Removed,
    Context,
}

impl Kind {
    pub fn marker(self) -> char {
        match self {
            Kind::Added => '+',
            Kind::Removed => '-',
            Kind::Context => ' ',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: Kind,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Diff {
    pub lines: Vec<DiffLine>,
    pub stats: Stats,
    pub changed: usize,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.stats.is_empty()
    }

    pub fn render(&self, max: usize) -> String {
        let mut out = String::new();
        let mut shown = 0usize;
        let mut skipped = 0usize;
        for line in &self.lines {
            if max > 0 && shown >= max {
                skipped += 1;
                continue;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            if line.text.starts_with("@@") {
                out.push_str(&line.text);
            } else {
                out.push(line.kind.marker());
                out.push(' ');
                out.push_str(&line.text);
            }
            shown += 1;
        }
        if skipped > 0 {
            out.push_str(&format!("\n… {skipped} ligne(s) masquée(s)"));
        }
        out
    }
}

pub const MODEL_MAX_LINES: usize = 12;

pub fn cap_diff_blocks(text: &str, max: usize) -> String {
    if !text.contains("```diff") {
        return text.to_string();
    }
    let mut out: Vec<String> = Vec::new();
    let mut in_diff = false;
    let mut shown = 0usize;
    let mut omitted = 0usize;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            if in_diff {
                if omitted > 0 {
                    out.push(format!("… {omitted} ligne(s) de diff omises"));
                    omitted = 0;
                }
                in_diff = false;
            } else if trimmed.trim_start_matches("```").trim() == "diff" {
                in_diff = true;
                shown = 0;
            }
            out.push(line.to_string());
            continue;
        }
        if in_diff {
            if shown < max {
                out.push(line.to_string());
                shown += 1;
            } else {
                omitted += 1;
            }
        } else {
            out.push(line.to_string());
        }
    }
    if omitted > 0 {
        out.push(format!("… {omitted} ligne(s) de diff omises"));
    }
    out.join("\n")
}

fn normalize(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .lines()
        .map(|l| l.to_string())
        .collect()
}

pub fn unified(old: &str, new: &str) -> Diff {
    let a = normalize(old);
    let b = normalize(new);

    let mut start = 0usize;
    while start < a.len() && start < b.len() && a[start] == b[start] {
        start += 1;
    }
    let mut end = 0usize;
    while end + start < a.len()
        && end + start < b.len()
        && a[a.len() - 1 - end] == b[b.len() - 1 - end]
    {
        end += 1;
    }
    let mid_a = &a[start..a.len() - end];
    let mid_b = &b[start..b.len() - end];
    if mid_a.is_empty() && mid_b.is_empty() {
        return Diff::default();
    }

    let mid: Vec<(Kind, String)> = if mid_a.len() > MAX_DP_LINES || mid_b.len() > MAX_DP_LINES {
        let mut ops = Vec::with_capacity(mid_a.len() + mid_b.len());
        ops.extend(mid_a.iter().map(|l| (Kind::Removed, l.clone())));
        ops.extend(mid_b.iter().map(|l| (Kind::Added, l.clone())));
        ops
    } else {
        lcs_ops(mid_a, mid_b)
    };

    let mut ops: Vec<(Kind, String)> = Vec::with_capacity(a.len() + mid.len());
    ops.extend(a[..start].iter().map(|l| (Kind::Context, l.clone())));
    ops.extend(mid);
    ops.extend(
        a[a.len() - end..]
            .iter()
            .map(|l| (Kind::Context, l.clone())),
    );

    let mut stats = Stats::default();
    let mut changed = 0usize;
    for (kind, _) in &ops {
        match kind {
            Kind::Added => stats.added += 1,
            Kind::Removed => stats.removed += 1,
            Kind::Context => {}
        }
        if *kind != Kind::Context {
            changed += 1;
        }
    }

    let mut old_pos = Vec::with_capacity(ops.len() + 1);
    let mut new_pos = Vec::with_capacity(ops.len() + 1);
    let (mut o, mut n) = (0usize, 0usize);
    for (kind, _) in &ops {
        old_pos.push(o);
        new_pos.push(n);
        match kind {
            Kind::Added => n += 1,
            Kind::Removed => o += 1,
            Kind::Context => {
                o += 1;
                n += 1;
            }
        }
    }
    old_pos.push(o);
    new_pos.push(n);

    let mut groups: Vec<(usize, usize)> = Vec::new();
    for (i, (kind, _)) in ops.iter().enumerate() {
        if *kind == Kind::Context {
            continue;
        }
        let from = i.saturating_sub(CONTEXT);
        let to = (i + CONTEXT).min(ops.len().saturating_sub(1));
        match groups.last_mut() {
            Some(g) if from <= g.1 + 1 => g.1 = g.1.max(to),
            _ => groups.push((from, to)),
        }
    }

    let mut lines = Vec::new();
    for (from, to) in groups {
        let old_count = old_pos[to + 1] - old_pos[from];
        let new_count = new_pos[to + 1] - new_pos[from];
        let old_start = if old_count == 0 {
            old_pos[from]
        } else {
            old_pos[from] + 1
        };
        let new_start = if new_count == 0 {
            new_pos[from]
        } else {
            new_pos[from] + 1
        };
        lines.push(DiffLine {
            kind: Kind::Context,
            text: format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@"),
        });
        for (kind, text) in &ops[from..=to] {
            lines.push(DiffLine {
                kind: *kind,
                text: text.clone(),
            });
        }
    }

    Diff {
        lines,
        stats,
        changed,
    }
}

fn lcs_ops(a: &[String], b: &[String]) -> Vec<(Kind, String)> {
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut ops = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            ops.push((Kind::Context, a[i].clone()));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            ops.push((Kind::Removed, a[i].clone()));
            i += 1;
        } else {
            ops.push((Kind::Added, b[j].clone()));
            j += 1;
        }
    }
    ops.extend(a[i..].iter().map(|l| (Kind::Removed, l.clone())));
    ops.extend(b[j..].iter().map(|l| (Kind::Added, l.clone())));
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(d: &Diff) -> Vec<(char, String)> {
        d.lines
            .iter()
            .map(|l| (l.kind.marker(), l.text.clone()))
            .collect()
    }

    #[test]
    fn identical_text_has_no_diff() {
        let d = unified("a\nb\nc\n", "a\nb\nc\n");
        assert!(d.is_empty());
        assert!(d.lines.is_empty());
        assert_eq!(d.stats.label(), "");
    }

    #[test]
    fn one_added_line_is_reported_with_context() {
        let d = unified("a\nb\nc\n", "a\nb\nNEW\nc\n");
        assert_eq!(
            d.stats,
            Stats {
                added: 1,
                removed: 0
            }
        );
        let k = kinds(&d);
        assert_eq!(k[0].0, ' ', "l'en-tête @@ est une ligne de contexte");
        assert!(k[0].1.starts_with("@@ -"), "{:?}", k[0]);
        assert!(k.iter().any(|(m, t)| *m == '+' && t == "NEW"));
        assert!(
            k.iter().any(|(m, t)| *m == ' ' && t == "b"),
            "contexte absent"
        );
    }

    #[test]
    fn replacement_shows_both_sides() {
        let d = unified("x = 1\ny = 2\n", "x = 1\ny = 3\n");
        assert_eq!(
            d.stats,
            Stats {
                added: 1,
                removed: 1
            }
        );
        let k = kinds(&d);
        assert!(k.iter().any(|(m, t)| *m == '-' && t == "y = 2"));
        assert!(k.iter().any(|(m, t)| *m == '+' && t == "y = 3"));
    }

    #[test]
    fn crlf_rewrite_of_the_same_text_is_not_a_change() {
        let d = unified("a\nb\nc\n", "a\r\nb\r\nc\r\n");
        assert!(
            d.is_empty(),
            "CRLF ne doit pas produire de faux diff : {d:?}"
        );
    }

    #[test]
    fn unchanged_head_and_tail_do_not_appear() {
        let old = (1..=50)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let new = old.replace("l25", "l25-modifiee");
        let d = unified(&old, &new);
        assert_eq!(
            d.stats,
            Stats {
                added: 1,
                removed: 1
            }
        );
        assert!(d.lines.len() <= 10, "trop de lignes : {}", d.lines.len());
        let out = d.render(0);
        assert!(out.contains("l25-modifiee"));
        assert!(
            !out.contains("l1\n"),
            "la tête du fichier ne doit pas apparaître"
        );
    }

    #[test]
    fn render_caps_and_says_how_many_lines_were_hidden() {
        let new = (1..=40)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let d = unified("", &new);
        assert_eq!(d.stats.added, 40);
        let out = d.render(10);
        let body: Vec<&str> = out.lines().filter(|l| !l.starts_with('…')).collect();
        assert_eq!(body.len(), 10, "plafond non respecté : {out}");
        assert!(out.contains("31 ligne(s) masquée(s)"), "{out}");
        assert!(
            out.starts_with("@@ -0,0 +1,40 @@"),
            "en-tête absent : {out}"
        );
    }

    #[test]
    fn only_diff_fences_are_capped() {
        let text = "Remplacement (+3 / -1)\n```rust\nfn a() {}\n```\n```diff\n@@ -1,2 +1,2 @@\n a\n-b\n+c\n```\nfin";
        let capped = cap_diff_blocks(text, 2);
        assert!(capped.contains("@@ -1,2 +1,2 @@\n a"), "{capped}");
        assert!(capped.contains("2 ligne(s) de diff omises"), "{capped}");
        assert!(
            !capped.contains("+c"),
            "ligne au-delà du plafond conservée : {capped}"
        );
        assert!(capped.contains("fn a() {}"), "{capped}");
        assert!(capped.starts_with("Remplacement (+3 / -1)"), "{capped}");
        assert!(capped.ends_with("fin"), "{capped}");
    }

    #[test]
    fn text_without_diff_is_returned_unchanged() {
        let text = "sortie d'outil banale\nligne 2";
        assert_eq!(cap_diff_blocks(text, 12), text);
    }

    #[test]
    fn cap_under_the_limit_changes_nothing() {
        let text = "```diff\n@@ -1 +1 @@\n-a\n+b\n```";
        assert_eq!(cap_diff_blocks(text, 12), text);
    }

    #[test]
    fn rewrite_is_named_as_such() {
        let big = Stats {
            added: 40,
            removed: 38,
        };
        assert!(big.looks_like_rewrite());
        let small = Stats {
            added: 4,
            removed: 2,
        };
        assert!(!small.looks_like_rewrite());
        let add = Stats {
            added: 60,
            removed: 0,
        };
        assert!(!add.looks_like_rewrite());
    }

    #[test]
    fn huge_block_replacement_still_works_without_blowing_memory() {
        let old = (1..=3000)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let new = (1..=3000)
            .map(|i| format!("b{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let d = unified(&old, &new);
        assert_eq!(
            d.stats,
            Stats {
                added: 3000,
                removed: 3000
            }
        );
        assert!(!d.lines.is_empty());
    }
}
