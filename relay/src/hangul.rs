const CHOSEONG: [char; 19] = [
    'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ',
    'ㅌ', 'ㅍ', 'ㅎ',
];
const JUNGSEONG: [char; 21] = [
    'ㅏ', 'ㅐ', 'ㅑ', 'ㅒ', 'ㅓ', 'ㅔ', 'ㅕ', 'ㅖ', 'ㅗ', 'ㅘ', 'ㅙ', 'ㅚ', 'ㅛ', 'ㅜ', 'ㅝ', 'ㅞ',
    'ㅟ', 'ㅠ', 'ㅡ', 'ㅢ', 'ㅣ',
];
// Index 0 is "no jongseong" (a space placeholder, matching the Python reference).
const JONGSEONG: [char; 28] = [
    ' ', 'ㄱ', 'ㄲ', 'ㄳ', 'ㄴ', 'ㄵ', 'ㄶ', 'ㄷ', 'ㄹ', 'ㄺ', 'ㄻ', 'ㄼ', 'ㄽ', 'ㄾ', 'ㄿ', 'ㅀ',
    'ㅁ', 'ㅂ', 'ㅄ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
];

const SBASE: u32 = 0xAC00;
const VCOUNT: u32 = 21;
const TCOUNT: u32 = 28;

fn key_to_cho(key: char) -> Option<char> {
    Some(match key {
        'r' => 'ㄱ', 'R' => 'ㄲ', 's' => 'ㄴ', 'e' => 'ㄷ', 'E' => 'ㄸ', 'f' => 'ㄹ',
        'a' => 'ㅁ', 'q' => 'ㅂ', 'Q' => 'ㅃ', 't' => 'ㅅ', 'T' => 'ㅆ', 'd' => 'ㅇ',
        'w' => 'ㅈ', 'W' => 'ㅉ', 'c' => 'ㅊ', 'z' => 'ㅋ', 'x' => 'ㅌ', 'v' => 'ㅍ', 'g' => 'ㅎ',
        _ => return None,
    })
}

fn key_to_jong_consonant(key: char) -> Option<char> {
    Some(match key {
        'r' => 'ㄱ', 'R' => 'ㄲ', 's' => 'ㄴ', 'e' => 'ㄷ', 'f' => 'ㄹ',
        'a' => 'ㅁ', 'q' => 'ㅂ', 't' => 'ㅅ', 'T' => 'ㅆ', 'd' => 'ㅇ',
        'w' => 'ㅈ', 'c' => 'ㅊ', 'z' => 'ㅋ', 'x' => 'ㅌ', 'v' => 'ㅍ', 'g' => 'ㅎ',
        _ => return None,
    })
}

fn key_to_jung(key: char) -> Option<char> {
    Some(match key {
        'k' => 'ㅏ', 'o' => 'ㅐ', 'i' => 'ㅑ', 'O' => 'ㅒ', 'j' => 'ㅓ', 'p' => 'ㅔ',
        'u' => 'ㅕ', 'P' => 'ㅖ', 'h' => 'ㅗ', 'y' => 'ㅛ', 'n' => 'ㅜ', 'b' => 'ㅠ',
        'm' => 'ㅡ', 'l' => 'ㅣ',
        _ => return None,
    })
}

fn jung_combo(a: char, b: char) -> Option<char> {
    Some(match (a, b) {
        ('ㅗ', 'ㅏ') => 'ㅘ', ('ㅗ', 'ㅐ') => 'ㅙ', ('ㅗ', 'ㅣ') => 'ㅚ',
        ('ㅜ', 'ㅓ') => 'ㅝ', ('ㅜ', 'ㅔ') => 'ㅞ', ('ㅜ', 'ㅣ') => 'ㅟ',
        ('ㅡ', 'ㅣ') => 'ㅢ',
        _ => return None,
    })
}

fn jong_combo(a: char, b: char) -> Option<char> {
    Some(match (a, b) {
        ('ㄱ', 'ㅅ') => 'ㄳ', ('ㄴ', 'ㅈ') => 'ㄵ', ('ㄴ', 'ㅎ') => 'ㄶ',
        ('ㄹ', 'ㄱ') => 'ㄺ', ('ㄹ', 'ㅁ') => 'ㄻ', ('ㄹ', 'ㅂ') => 'ㄼ',
        ('ㄹ', 'ㅅ') => 'ㄽ', ('ㄹ', 'ㅌ') => 'ㄾ', ('ㄹ', 'ㅍ') => 'ㄿ',
        ('ㄹ', 'ㅎ') => 'ㅀ', ('ㅂ', 'ㅅ') => 'ㅄ',
        _ => return None,
    })
}

fn jung_combo_parts(combo: char) -> Option<(char, char)> {
    let table = [
        ('ㅘ', ('ㅗ', 'ㅏ')), ('ㅙ', ('ㅗ', 'ㅐ')), ('ㅚ', ('ㅗ', 'ㅣ')),
        ('ㅝ', ('ㅜ', 'ㅓ')), ('ㅞ', ('ㅜ', 'ㅔ')), ('ㅟ', ('ㅜ', 'ㅣ')),
        ('ㅢ', ('ㅡ', 'ㅣ')),
    ];
    table.iter().find(|(c, _)| *c == combo).map(|(_, p)| *p)
}

fn jong_combo_parts(combo: char) -> Option<(char, char)> {
    let table = [
        ('ㄳ', ('ㄱ', 'ㅅ')), ('ㄵ', ('ㄴ', 'ㅈ')), ('ㄶ', ('ㄴ', 'ㅎ')),
        ('ㄺ', ('ㄹ', 'ㄱ')), ('ㄻ', ('ㄹ', 'ㅁ')), ('ㄼ', ('ㄹ', 'ㅂ')),
        ('ㄽ', ('ㄹ', 'ㅅ')), ('ㄾ', ('ㄹ', 'ㅌ')), ('ㄿ', ('ㄹ', 'ㅍ')),
        ('ㅀ', ('ㄹ', 'ㅎ')), ('ㅄ', ('ㅂ', 'ㅅ')),
    ];
    table.iter().find(|(c, _)| *c == combo).map(|(_, p)| *p)
}

fn compose_syllable(cho: Option<char>, jung: Option<char>, jong: Option<char>) -> String {
    match (cho, jung) {
        (_, None) => cho.map(|c| c.to_string()).unwrap_or_default(),
        (None, Some(j)) => j.to_string(),
        (Some(c), Some(j)) => {
            let l = CHOSEONG.iter().position(|&x| x == c);
            let v = JUNGSEONG.iter().position(|&x| x == j);
            let t = match jong {
                None => Some(0),
                Some(t) => JONGSEONG.iter().position(|&x| x == t),
            };
            if let (Some(l), Some(v), Some(t)) = (l, v, t) {
                let code = SBASE + (l as u32 * VCOUNT + v as u32) * TCOUNT + t as u32;
                char::from_u32(code).map(|ch| ch.to_string()).unwrap_or_default()
            } else {
                // Not composable jamo — emit the raw characters instead of panicking.
                let mut s = String::new();
                s.push(c);
                s.push(j);
                if let Some(t) = jong {
                    s.push(t);
                }
                s
            }
        }
    }
}

fn decompose_syllable(ch: char) -> Option<(usize, usize, usize)> {
    let code = ch as u32;
    if (0xAC00..=0xD7A3).contains(&code) {
        let s = code - SBASE;
        Some((
            (s / (VCOUNT * TCOUNT)) as usize,
            ((s / TCOUNT) % VCOUNT) as usize,
            (s % TCOUNT) as usize,
        ))
    } else {
        None
    }
}

fn compose_indices(l: usize, v: usize, t: usize) -> char {
    char::from_u32(SBASE + (l as u32 * VCOUNT + v as u32) * TCOUNT + t as u32).unwrap_or(' ')
}

fn cho_idx(c: char) -> usize {
    CHOSEONG.iter().position(|&x| x == c).unwrap_or(11)
}
fn jong_idx(c: char) -> usize {
    JONGSEONG.iter().position(|&x| x == c).unwrap_or(0)
}

/// 대표음 of a compound jongseong (word-final / before a consonant).
fn representative_jong(compound: char) -> char {
    match compound {
        'ㄳ' | 'ㄺ' => 'ㄱ',
        'ㄵ' | 'ㄶ' => 'ㄴ',
        'ㄻ' => 'ㅁ',
        'ㄼ' | 'ㄽ' | 'ㄾ' | 'ㅀ' => 'ㄹ',
        'ㄿ' | 'ㅄ' => 'ㅂ',
        other => other,
    }
}

/// 연음 split of a compound jongseong before a vowel: (jong left on this
/// syllable, onset carried to the next). ㅎ-clusters drop the ㅎ.
fn liaison_split(compound: char) -> (Option<char>, Option<char>) {
    match compound {
        'ㄺ' => (Some('ㄹ'), Some('ㄱ')),
        'ㄻ' => (Some('ㄹ'), Some('ㅁ')),
        'ㄼ' => (Some('ㄹ'), Some('ㅂ')),
        'ㄽ' => (Some('ㄹ'), Some('ㅅ')),
        'ㄾ' => (Some('ㄹ'), Some('ㅌ')),
        'ㄿ' => (Some('ㄹ'), Some('ㅍ')),
        'ㄳ' => (Some('ㄱ'), Some('ㅅ')),
        'ㄵ' => (Some('ㄴ'), Some('ㅈ')),
        'ㅄ' => (Some('ㅂ'), Some('ㅅ')),
        'ㄶ' => (None, Some('ㄴ')),
        'ㅀ' => (None, Some('ㄹ')),
        other => (Some(other), None),
    }
}

/// Normalize ONLY 겹받침 (compound final consonants), which the model gets wrong
/// on its own (닭→딸): 대표음 word-finally / before a consonant (닭→닥, 칡→칙,
/// 밝기→박기) and 연음 before a vowel (닭이→달기, 많이→마니). Single codas / ㅎ /
/// liaison are LEFT ALONE — the model's own G2P handles them, and feeding it the
/// 대표음 spelling for those (꽃→꼳, 좋다→조타) is out-of-distribution text the
/// model never saw and makes it WORSE, not better.
fn korean_g2p(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut d: Vec<Option<(usize, usize, usize)>> =
        chars.iter().map(|&c| decompose_syllable(c)).collect();
    for i in 0..d.len() {
        let Some((_, _, t)) = d[i] else { continue };
        if t == 0 {
            continue;
        }
        let coda = JONGSEONG[t];
        if jong_combo_parts(coda).is_none() {
            continue; // only compound codas
        }
        let next_vowel = d.get(i + 1).copied().flatten().map(|(nl, _, _)| nl == 11).unwrap_or(false);
        if next_vowel {
            let (left, moved) = liaison_split(coda);
            if let Some(cell) = d[i].as_mut() {
                cell.2 = left.map(jong_idx).unwrap_or(0);
            }
            if let (Some(o), Some((_, nv, nt))) = (moved, d[i + 1]) {
                d[i + 1] = Some((cho_idx(o), nv, nt));
            }
        } else if let Some(cell) = d[i].as_mut() {
            cell.2 = jong_idx(representative_jong(coda));
        }
    }
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| match d[i] {
            Some((l, v, t)) => compose_indices(l, v, t),
            None => c,
        })
        .collect()
}

include!("hangul_common.rs");

fn common_set() -> &'static [u64; 175] {
    use std::sync::OnceLock;
    static SET: OnceLock<[u64; 175]> = OnceLock::new();
    SET.get_or_init(|| {
        let mut bits = [0u64; 175];
        for c in COMMON_SYLLABLES.chars() {
            let i = (c as u32 - SBASE) as usize;
            bits[i / 64] |= 1 << (i % 64);
        }
        bits
    })
}

fn is_common(c: char) -> bool {
    let code = c as u32;
    if !(0xAC00..=0xD7A3).contains(&code) {
        return true; // not a syllable — nothing to gate
    }
    let i = (code - SBASE) as usize;
    common_set()[i / 64] & (1 << (i % 64)) != 0
}

/// Full-coda 대표음 (used only when re-spelling an out-of-inventory syllable).
fn terminal_jong(j: char) -> char {
    match j {
        'ㄲ' | 'ㅋ' => 'ㄱ',
        'ㅅ' | 'ㅆ' | 'ㅈ' | 'ㅊ' | 'ㅌ' | 'ㅎ' => 'ㄷ',
        'ㅍ' => 'ㅂ',
        other => representative_jong(other),
    }
}

/// Nearest common vowel: drop the glide / merge the near-identical pair.
fn simple_jung(v: char) -> char {
    match v {
        'ㅛ' => 'ㅗ',
        'ㅠ' => 'ㅜ',
        'ㅑ' => 'ㅏ',
        'ㅕ' => 'ㅓ',
        'ㅒ' => 'ㅐ',
        'ㅖ' => 'ㅔ',
        'ㅝ' => 'ㅓ',
        'ㅞ' => 'ㅔ',
        'ㅘ' => 'ㅏ',
        'ㅙ' => 'ㅐ',
        'ㅢ' => 'ㅣ',
        'ㅚ' => 'ㅔ',
        'ㅟ' => 'ㅣ',
        other => other,
    }
}

fn plain_cho(l: char) -> char {
    match l {
        'ㄲ' => 'ㄱ',
        'ㄸ' => 'ㄷ',
        'ㅃ' => 'ㅂ',
        'ㅆ' => 'ㅅ',
        'ㅉ' => 'ㅈ',
        other => other,
    }
}

/// Replace a syllable outside the common inventory with its nearest common
/// pronunciation (the model garbles what it never saw: 쀀→뻑, 뵥→복, 휛→휙).
/// Ladder is closest-first: 대표음 coda → simplified vowel → both → plain onset →
/// dropped coda. Common syllables pass through untouched.
fn nearest_common(c: char) -> char {
    if is_common(c) {
        return c;
    }
    let Some((l, v, t)) = decompose_syllable(c) else { return c };
    let (lj, vj) = (CHOSEONG[l], JUNGSEONG[v]);
    let tr = if t == 0 { 0 } else { jong_idx(terminal_jong(JONGSEONG[t])) };
    let vr = JUNGSEONG.iter().position(|&x| x == simple_jung(vj)).unwrap_or(v);
    let lr = CHOSEONG.iter().position(|&x| x == plain_cho(lj)).unwrap_or(l);
    for cand in [
        compose_indices(l, v, tr),
        compose_indices(l, vr, t),
        compose_indices(l, vr, tr),
        compose_indices(lr, v, tr),
        compose_indices(lr, vr, tr),
        compose_indices(l, v, 0),
        compose_indices(l, vr, 0),
        compose_indices(lr, vr, 0),
    ] {
        if is_common(cand) {
            return cand;
        }
    }
    c
}

fn ood_gate(text: &str) -> String {
    text.chars().map(nearest_common).collect()
}

/// Merge the vowels that modern Korean pronounces identically: ㅒ→ㅐ always, and
/// ㅖ→ㅔ after a consonant other than ㄹ (계→게; 예·례 keep ㅖ). Rescues rare/typo
/// syllables (a held Shift turns 깨 into 꺠) into a form the model can actually say.
fn merge_vowels(text: &str) -> String {
    text.chars()
        .map(|c| match decompose_syllable(c) {
            Some((l, 3, t)) => compose_indices(l, 1, t), // ㅒ→ㅐ
            Some((l, 7, t)) if l != 11 && l != 5 => compose_indices(l, 5, t), // ㅖ→ㅔ
            _ => c,
        })
        .collect()
}

/// Text as the TTS should pronounce it (applied by the worker just before synth,
/// NOT at input, so the queue overlay keeps the raw typed text): 겹받침 → 대표음
/// / 연음 (닭→닥, 닭이→달기), merged vowels (꺠→깨), rare/typo syllables snapped to
/// the nearest common pronunciation (쀀→뻑), and lone jamo made speakable.
pub fn for_speech(text: &str) -> String {
    let text = ood_gate(&merge_vowels(&korean_g2p(text)));
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if CHOSEONG.contains(&ch) {
            out.push_str(&compose_syllable(Some(ch), Some('ㅡ'), None));
        } else if JUNGSEONG.contains(&ch) {
            out.push_str(&compose_syllable(Some('ㅇ'), Some(ch), None));
        } else {
            out.push(ch);
        }
    }
    out
}

pub struct HangulComposer {
    committed: String,
    cho: Option<char>,
    jung: Option<char>,
    jong: Option<char>,
}

impl Default for HangulComposer {
    fn default() -> Self {
        Self::new()
    }
}

impl HangulComposer {
    pub fn new() -> Self {
        Self { committed: String::new(), cho: None, jung: None, jong: None }
    }

    fn current_syllable(&self) -> String {
        compose_syllable(self.cho, self.jung, self.jong)
    }

    fn commit_current(&mut self) {
        if self.cho.is_some() || self.jung.is_some() {
            self.committed.push_str(&self.current_syllable());
        }
        self.cho = None;
        self.jung = None;
        self.jong = None;
    }

    pub fn text(&self) -> String {
        format!("{}{}", self.committed, self.current_syllable())
    }

    /// Drop syllables the model cannot pronounce, returning the first one removed
    /// so the caller can say why. Only COMMITTED text is checked — the syllable
    /// still being composed may yet gain a jongseong and become a different one.
    pub fn strip_blocked(&mut self, blocked: &str) -> Option<char> {
        if blocked.is_empty() {
            return None;
        }
        let hit = self.committed.chars().find(|c| blocked.contains(*c))?;
        self.committed.retain(|c| !blocked.contains(c));
        Some(hit)
    }

    pub fn feed_key(&mut self, key: char) -> String {
        // A Shift held over from the previous keystroke must not turn a jamo key
        // into a literal English capital. Only ㄲㄸㅃㅆㅉ / ㅒㅖ have a shifted
        // jamo (their uppercase key); any other uppercase letter falls back to
        // its base jamo instead of leaking English.
        let key = if key.is_ascii_uppercase()
            && key_to_cho(key).is_none()
            && key_to_jung(key).is_none()
        {
            key.to_ascii_lowercase()
        } else {
            key
        };
        if let Some(cho_jamo) = key_to_cho(key) {
            self.feed_consonant(key, cho_jamo);
        } else if let Some(jung_jamo) = key_to_jung(key) {
            self.feed_vowel(jung_jamo);
        } else {
            self.commit_current();
            self.committed.push(key);
        }
        self.text()
    }

    /// Feeds a key as a literal character (English mode) — no dubeolsik
    /// jamo mapping, just commits any in-progress syllable and appends as-is.
    pub fn feed_literal(&mut self, key: char) -> String {
        self.commit_current();
        self.committed.push(key);
        self.text()
    }

    /// Inserts an already-formed string (clipboard paste): commit any in-progress
    /// syllable, then append verbatim — pasted Hangul is precomposed, not jamo.
    pub fn feed_str(&mut self, s: &str) -> String {
        self.commit_current();
        self.committed.push_str(s);
        self.text()
    }

    fn feed_consonant(&mut self, key: char, cho_jamo: char) {
        let jong_jamo = key_to_jong_consonant(key);

        if self.cho.is_none() {
            self.cho = Some(cho_jamo);
            return;
        }

        if self.jung.is_none() {
            self.commit_current();
            self.cho = Some(cho_jamo);
            return;
        }

        if self.jong.is_none() {
            match jong_jamo {
                None => {
                    self.commit_current();
                    self.cho = Some(cho_jamo);
                }
                Some(j) => self.jong = Some(j),
            }
            return;
        }

        match jong_jamo.and_then(|j| jong_combo(self.jong.unwrap(), j)) {
            Some(combo) => self.jong = Some(combo),
            None => {
                self.commit_current();
                self.cho = Some(cho_jamo);
            }
        }
    }

    fn feed_vowel(&mut self, jung_jamo: char) {
        if self.cho.is_none() && self.jung.is_none() {
            self.jung = Some(jung_jamo);
            return;
        }

        if let Some(jong) = self.jong {
            // Compound jongseong (ㄳ, ㄵ, ...) splits on steal-back: the first
            // part stays as the current syllable's (simple) jongseong, only
            // the second part moves to become the next syllable's choseong.
            // A simple jongseong moves across whole, leaving none behind.
            match jong_combo_parts(jong) {
                Some((keep, steal)) => {
                    self.jong = Some(keep);
                    self.commit_current();
                    self.cho = Some(steal);
                }
                None => {
                    self.jong = None;
                    self.commit_current();
                    self.cho = Some(jong);
                }
            }
            self.jung = Some(jung_jamo);
            return;
        }

        match self.jung {
            None => self.jung = Some(jung_jamo),
            Some(existing) => match jung_combo(existing, jung_jamo) {
                Some(combo) => self.jung = Some(combo),
                None => {
                    self.commit_current();
                    self.jung = Some(jung_jamo);
                }
            },
        }
    }

    pub fn backspace(&mut self) -> String {
        if let Some(jong) = self.jong {
            self.jong = jong_combo_parts(jong).map(|(a, _)| a);
        } else if let Some(jung) = self.jung {
            match jung_combo_parts(jung) {
                Some((a, _)) => self.jung = Some(a),
                None => self.jung = None,
            }
        } else if self.cho.is_some() {
            self.cho = None;
        } else if !self.committed.is_empty() {
            self.committed.pop();
        }
        self.text()
    }

    /// Commit and return the RAW typed text (what the queue overlay shows).
    /// Pronunciation normalization for TTS happens later via [`for_speech`].
    pub fn finalize(&mut self) -> String {
        self.commit_current();
        std::mem::take(&mut self.committed)
    }

    pub fn reset(&mut self) {
        self.committed.clear();
        self.cho = None;
        self.jung = None;
        self.jong = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reverse_cho(jamo: char) -> char {
        "rRsEeEfaQqTtdWwczxvg"
            .chars()
            .find(|&k| key_to_cho(k) == Some(jamo))
            .unwrap()
    }

    fn reverse_jung(jamo: char) -> char {
        "koiOjpuPhynbml"
            .chars()
            .find(|&k| key_to_jung(k) == Some(jamo))
            .unwrap()
    }

    fn reverse_jong_consonant(jamo: char) -> char {
        "rRsefaqtTdwczxvg"
            .chars()
            .find(|&k| key_to_jong_consonant(k) == Some(jamo))
            .unwrap()
    }

    fn decompose_word_to_keys(word: &str) -> Vec<char> {
        let mut keys = Vec::new();
        for ch in word.chars() {
            let code = ch as u32;
            if (0xAC00..=0xD7A3).contains(&code) {
                let idx = code - SBASE;
                let t_index = idx % TCOUNT;
                let v_index = (idx / TCOUNT) % VCOUNT;
                let l_index = idx / (TCOUNT * VCOUNT);

                keys.push(reverse_cho(CHOSEONG[l_index as usize]));

                let jung = JUNGSEONG[v_index as usize];
                match jung_combo_parts(jung) {
                    Some((a, b)) => {
                        keys.push(reverse_jung(a));
                        keys.push(reverse_jung(b));
                    }
                    None => keys.push(reverse_jung(jung)),
                }

                if t_index > 0 {
                    let jong = JONGSEONG[t_index as usize];
                    match jong_combo_parts(jong) {
                        Some((a, b)) => {
                            keys.push(reverse_jong_consonant(a));
                            keys.push(reverse_jong_consonant(b));
                        }
                        None => keys.push(reverse_jong_consonant(jong)),
                    }
                }
            } else {
                keys.push(ch);
            }
        }
        keys
    }

    fn roundtrip(word: &str) -> bool {
        let keys = decompose_word_to_keys(word);
        let mut c = HangulComposer::new();
        for k in &keys {
            c.feed_key(*k);
        }
        let result = c.finalize();
        let ok = result == word;
        println!(
            "{} {:?} -> {} -> {:?}",
            if ok { "OK  " } else { "FAIL" },
            word,
            keys.iter().collect::<String>(),
            result
        );
        ok
    }

    #[test]
    fn matches_python_reference_words() {
        let words = [
            "안녕하세요", "서하은", "메이븐", "힐러", "전투력", "생활력", "매력",
            "감사합니다", "닭", "값", "앉다", "많다", "긁다", "밟다", "훑다",
            "왕", "궤도", "의사", "돼지", "뷁", "쉼표", "빛", "꽃", "닮",
        ];
        let mut all_ok = true;
        for w in words {
            if !roundtrip(w) {
                all_ok = false;
            }
        }
        assert!(all_ok, "some roundtrip cases failed");
    }

    #[test]
    fn backspace_decomposes_step_by_step() {
        let mut c = HangulComposer::new();
        // 닭 = e(ㄷ) k(ㅏ) f(ㄹ) r(ㄱ) -> jongseong ㄺ
        c.feed_key('e');
        c.feed_key('k');
        c.feed_key('f');
        c.feed_key('r');
        assert_eq!(c.text(), "닭");
        c.backspace(); // ㄺ decomposes to its first part: ㄹ
        assert_eq!(c.text(), "달");
        c.backspace(); // ㄹ is not itself a combo -> jongseong cleared entirely
        assert_eq!(c.text(), "다");
        c.backspace(); // remove jungseong ㅏ
        assert_eq!(c.text(), "ㄷ");
        c.backspace(); // remove choseong
        assert_eq!(c.text(), "");
    }

    #[test]
    fn compound_jongseong_steal_back_splits_correctly() {
        let mut c = HangulComposer::new();
        // 갃 = r(ㄱ) k(ㅏ) r(ㄱ) t(ㅅ) -> jongseong ㄳ
        c.feed_key('r');
        c.feed_key('k');
        c.feed_key('r');
        c.feed_key('t');
        assert_eq!(c.text(), "갃");
        // Direct vowel steal-back (no consonant in between) used to panic:
        // it tried to use the whole compound ㄳ as a choseong.
        c.feed_key('k');
        assert_eq!(c.text(), "각사");

        let mut c2 = HangulComposer::new();
        // 안 = d(ㅇ) k(ㅏ) s(ㄴ) -> simple jongseong steals back whole.
        c2.feed_key('d');
        c2.feed_key('k');
        c2.feed_key('s');
        assert_eq!(c2.text(), "안");
        c2.feed_key('k');
        assert_eq!(c2.text(), "아나");
    }

    #[test]
    fn roundtrip_all_hangul_syllables() {
        // Every single Hangul syllable 가(U+AC00)..힣(U+D7A3): type its jamo via
        // the dubeolsik keys and confirm the composer rebuilds the exact syllable
        // (and never panics along the way).
        let mut failures = Vec::new();
        for code in 0xAC00u32..=0xD7A3 {
            let syl = char::from_u32(code).unwrap().to_string();
            let keys = decompose_word_to_keys(&syl);
            let mut c = HangulComposer::new();
            for k in &keys {
                c.feed_key(*k);
            }
            let got = c.finalize();
            if got != syl {
                failures.push((syl, keys.iter().collect::<String>(), got));
            }
        }
        for (syl, keys, got) in failures.iter().take(40) {
            println!("FAIL {syl} via {keys:?} -> {got:?}");
        }
        assert!(
            failures.is_empty(),
            "{} / 11172 syllables failed roundtrip",
            failures.len()
        );
        println!("all 11172 hangul syllables roundtrip OK");
    }

    #[test]
    fn all_syllables_then_vowel_steal_back_never_panics() {
        // The historical crash class: typing a syllable (esp. with a compound
        // jongseong like ㄳ/ㄺ) then a vowel, which steals the final consonant
        // into the next syllable. Exercise it for every syllable + ㅏ (key 'k')
        // and confirm no panic and the first char stays a valid syllable.
        for code in 0xAC00u32..=0xD7A3 {
            let syl = char::from_u32(code).unwrap();
            let keys = decompose_word_to_keys(&syl.to_string());
            let mut c = HangulComposer::new();
            for k in &keys {
                c.feed_key(*k);
            }
            c.feed_key('k'); // ㅏ — triggers jongseong steal-back if present
            let out = c.finalize();
            let first = out.chars().next().unwrap();
            let fc = first as u32;
            assert!(
                (0xAC00..=0xD7A3).contains(&fc),
                "syllable {syl} + vowel produced bad first char {first:?} (out {out:?})"
            );
        }
        println!("all 11172 syllables survive vowel steal-back");
    }

    #[test]
    fn lone_jamo_shows_raw_but_speaks_pronounceable() {
        // Overlay must show exactly what was typed (ㅋㅋㅋ); only the submitted
        // text for TTS becomes pronounceable (크크크).
        let mut c = HangulComposer::new();
        let mut shown = String::new();
        for _ in 0..3 {
            shown = c.feed_key('z'); // ㅋ
        }
        assert_eq!(shown, "ㅋㅋㅋ", "overlay shows raw jamo");
        let raw = c.finalize();
        assert_eq!(raw, "ㅋㅋㅋ", "finalize/queue keeps raw typed text");
        assert_eq!(for_speech(&raw), "크크크", "TTS gets pronounceable");

        let mut v = HangulComposer::new();
        let mut shown_v = String::new();
        for _ in 0..5 {
            shown_v = v.feed_key('k'); // ㅏ
        }
        assert_eq!(shown_v, "ㅏㅏㅏㅏㅏ");
        assert_eq!(for_speech(&v.finalize()), "아아아아아");

        let mut s = HangulComposer::new();
        assert_eq!(s.feed_key('r'), "ㄱ"); // shows raw
        assert_eq!(for_speech(&s.finalize()), "그"); // speaks 그
    }

    #[test]
    fn compound_final_consonant_g2p() {
        // 대표음 word-finally / before a consonant.
        assert_eq!(for_speech("닭"), "닥");
        assert_eq!(for_speech("칡"), "칙");
        assert_eq!(for_speech("밝기"), "박기");
        assert_eq!(for_speech("깞"), "깝");
        assert_eq!(for_speech("여덟"), "여덜");
        // 연음 before a vowel (compound codas only).
        assert_eq!(for_speech("닭이"), "달기");
        assert_eq!(for_speech("읽어"), "일거");
        assert_eq!(for_speech("많이"), "마니"); // ㅎ drops
        assert_eq!(for_speech("삶아"), "살마");
        // Single codas / ㅎ / liaison are left to the model's own G2P — feeding it
        // 대표음 spelling for these is OOD and makes it worse (꽃→꼳 garbled).
        assert_eq!(for_speech("꽃"), "꽃");
        assert_eq!(for_speech("부엌"), "부엌");
        assert_eq!(for_speech("좋다"), "좋다");
        assert_eq!(for_speech("밥을"), "밥을");
        assert_eq!(for_speech("사랑"), "사랑");
    }

    #[test]
    fn rare_syllables_snap_to_nearest_common() {
        for (w, e) in [
            ("쀀킹 이디엇", "뻑킹 이디엇"), // held-Shift typo of 뻑킹
            ("뵥", "복"), ("뾱", "뽁"), ("묙", "목"), ("쬭", "쪽"), ("횩", "혹"),
            ("툑", "톡"), ("쿅", "콕"), ("푝", "폭"), ("휛", "휙"), ("꺠", "깨"),
            ("우윀", "우웩"), ("먓", "맛"), ("뿨", "뻐"), ("쑉", "쏙"), ("꾝", "꼭"),
        ] {
            assert_eq!(for_speech(w), e, "{w}");
        }
        // in-inventory text stays byte-identical
        for w in ["안녕하세요, 반갑습니다.", "교육과 유튜브의 과자"] {
            assert_eq!(for_speech(w), w);
        }
    }

    #[test]
    fn shift_held_over_stays_korean() {
        // Uppercase key with no shifted jamo falls back to the base jamo, never
        // English: shift+k → ㅏ (→ lone → 아), and mid-syllable stays Korean.
        let mut a = HangulComposer::new();
        a.feed_key('K');
        assert_eq!(a.finalize(), "아");

        let mut b = HangulComposer::new();
        b.feed_key('r'); // ㄱ
        b.feed_key('K'); // stray shift on ㅏ
        assert_eq!(b.finalize(), "가");

        // Keys that DO have a shifted jamo still double correctly.
        let mut d = HangulComposer::new();
        d.feed_key('R'); // shift+r → ㄲ → lone → 끄
        assert_eq!(d.finalize(), "끄");
    }

    #[test]
    fn non_letter_keys_pass_through_literally() {
        // Only non-alphabetic keys hit the literal fallback — every a-z key
        // is claimed by the dubeolsik consonant/vowel tables.
        let mut c = HangulComposer::new();
        for ch in "123 !?".chars() {
            c.feed_key(ch);
        }
        assert_eq!(c.finalize(), "123 !?");
    }
}
