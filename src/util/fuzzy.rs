/// computes levenshtein edit distance between two strings.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let len_a = a_chars.len();
    let len_b = b_chars.len();

    let mut prev = (0..=len_b).collect::<Vec<usize>>();
    let mut curr = vec![0; len_b + 1];

    for i in 1..=len_a {
        curr[0] = i;
        for j in 1..=len_b {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev.copy_from_slice(&curr);
    }

    prev[len_b]
}

/// finds the closest candidate string to input if distance is within threshold.
pub fn suggest_candidate<'a>(input: &str, candidates: &[&'a str]) -> Option<&'a str> {
    candidates
        .iter()
        .filter_map(|&candidate| {
            let dist = edit_distance(input, candidate);
            if dist <= 3 && dist < input.chars().count().max(candidate.chars().count()) {
                Some((candidate, dist))
            } else {
                None
            }
        })
        .min_by_key(|(_, dist)| *dist)
        .map(|(c, _)| c)
}

/// fuzzy subsequence match with word boundary bonuses.
pub fn fuzzy_match(pattern: &str, text: &str) -> Option<i64> {
    if pattern.is_empty() {
        return Some(0);
    }
    let p_lower: Vec<char> = pattern.to_lowercase().chars().collect();
    let t_lower: Vec<char> = text.to_lowercase().chars().collect();

    let mut p_idx = 0;
    let mut score = 0i64;
    let mut consecutive = 0i64;

    for (t_idx, &t_ch) in t_lower.iter().enumerate() {
        if p_idx < p_lower.len() && t_ch == p_lower[p_idx] {
            let is_boundary = if t_idx == 0 {
                true
            } else {
                let prev = t_lower[t_idx - 1];
                prev == ' ' || prev == '_' || prev == '-' || prev == ':'
            };

            let mut char_score = 10i64;
            if is_boundary {
                char_score += 20;
            }
            if consecutive > 0 {
                char_score += consecutive * 5;
            }

            score += char_score;
            consecutive += 1;
            p_idx += 1;
        } else {
            consecutive = 0;
        }
    }

    if p_idx == p_lower.len() {
        Some(score - (text.chars().count() as i64))
    } else {
        None
    }
}
