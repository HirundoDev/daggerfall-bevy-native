//! Glob matching for the `[exclusions]` retail rule (DESIGN-v1 §4.4).
//!
//! Semantics (fixed by the F6-B pack):
//!
//! * `**` matches zero or more whole path segments;
//! * `*` matches any run of characters inside one segment (never `/`);
//! * `?` matches exactly one character inside one segment (never `/`);
//! * matching is **case-sensitive** byte matching.
//!
//! Patterns and paths are POSIX `/`-separated. Only files are matched, always
//! against their path relative to the scanned root.

/// True when `path` matches `pattern` under the §4.4 semantics.
pub fn glob_matches(pattern: &str, path: &str) -> bool {
    let p: Vec<&str> = pattern.split('/').collect();
    let s: Vec<&str> = path.split('/').collect();
    segments_match(&p, &s)
}

fn segments_match(pattern: &[&str], path: &[&str]) -> bool {
    let Some((head, tail)) = pattern.split_first() else {
        return path.is_empty();
    };
    if *head == "**" {
        // `**` swallows zero or more whole segments.
        for take in 0..=path.len() {
            if segments_match(tail, &path[take..]) {
                return true;
            }
        }
        false
    } else {
        !path.is_empty() && segment_matches(head, path[0]) && segments_match(tail, &path[1..])
    }
}

/// Classic single-segment wildcard match (`*` and `?`, no `/` involved).
fn segment_matches(pattern: &str, segment: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = segment.chars().collect();
    // dp[i][j] = p[i..] matches s[j..]
    let mut dp = vec![vec![false; s.len() + 1]; p.len() + 1];
    dp[p.len()][s.len()] = true;
    for i in (0..p.len()).rev() {
        for j in (0..=s.len()).rev() {
            let matched = match p[i] {
                '*' => {
                    // skip the star, or let it swallow one more char
                    dp[i + 1][j] || (j < s.len() && (dp[i + 1][j + 1] || dp[i][j + 1]))
                }
                '?' => j < s.len() && dp[i + 1][j + 1],
                c => j < s.len() && s[j] == c && dp[i + 1][j + 1],
            };
            dp[i][j] = matched;
        }
    }
    dp[0][0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_star_matches_zero_or_more_segments() {
        assert!(glob_matches("**/*.BSA", "retail.BSA"));
        assert!(glob_matches("**/*.BSA", "data/retail.BSA"));
        assert!(glob_matches("**/*.BSA", "a/b/c/retail.BSA"));
        assert!(!glob_matches("**/*.BSA", "data/retail.BSA.bak"));
        assert!(glob_matches("**/arena2/**", "arena2/MAP0.BSA"));
        assert!(glob_matches("**/arena2/**", "game/arena2/inner/file"));
        assert!(glob_matches("**/*.VID", "x.VID"));
        assert!(glob_matches("**/*.pal", "palettes/foo.pal"));
    }

    #[test]
    fn single_star_stays_inside_one_segment() {
        assert!(glob_matches("*.BSA", "retail.BSA"));
        assert!(!glob_matches("*.BSA", "data/retail.BSA"));
        assert!(glob_matches("arena2/*", "arena2/MAP0.BSA"));
        assert!(!glob_matches("arena2/*", "arena2/sub/MAP0.BSA"));
    }

    #[test]
    fn question_mark_is_exactly_one_char() {
        assert!(glob_matches("?.BSA", "a.BSA"));
        assert!(!glob_matches("?.BSA", "ab.BSA"));
        assert!(!glob_matches("?.BSA", ".BSA"));
        assert!(glob_matches("a?c", "abc"));
        assert!(!glob_matches("a?c", "ac"));
    }

    #[test]
    fn matching_is_case_sensitive() {
        assert!(glob_matches("**/ARENA2/**", "ARENA2/MAP0.BSA"));
        assert!(!glob_matches("**/ARENA2/**", "arena2/MAP0.BSA"));
        assert!(!glob_matches("**/*.BSA", "retail.bsa"));
        assert!(glob_matches("**/*.pal", "UI/palette.pal"));
        assert!(!glob_matches("**/*.PAL", "UI/palette.pal"));
    }

    #[test]
    fn full_pack_list() {
        let globs = [
            "**/*.BSA",
            "**/*.VID",
            "**/*.pal",
            "**/arena2/**",
            "**/ARENA2/**",
        ];
        assert!(globs.iter().any(|g| glob_matches(g, "ARENA2/X.BSA")));
        assert!(globs.iter().any(|g| glob_matches(g, "arena2/x.pal")));
        assert!(!globs
            .iter()
            .any(|g| glob_matches(g, "chapters/10-world-model.md")));
        assert!(!globs.iter().any(|g| glob_matches(g, "CONTRACT.json")));
    }
}
