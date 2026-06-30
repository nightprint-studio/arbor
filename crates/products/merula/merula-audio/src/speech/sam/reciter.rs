//! Reciter — faithful 1:1 port of `discordier/sam-js` (`src/reciter/*.es6`).
//! Converts English text into a SAM phoneme/stress string using the original
//! rule set. The rule strings and char-flag table are the SAM ROM data.

use std::collections::HashMap;
use std::sync::OnceLock;

// ── Char flags (reciter) ─────────────────────────────────────────────────────
const FLAG_NUMERIC: u8 = 0x01;
const FLAG_RULESET2: u8 = 0x02;
const FLAG_VOICED: u8 = 0x04;
const FLAG_0X08: u8 = 0x08;
const FLAG_DIPHTHONG: u8 = 0x10;
const FLAG_CONSONANT: u8 = 0x20;
const FLAG_VOWEL_OR_Y: u8 = 0x40;
const FLAG_ALPHA_OR_QUOT: u8 = 0x80;

/// Reciter char-flag table (`charFlags`). Unknown chars → 0.
fn cf(c: char) -> u8 {
    match c {
        'A' | 'E' | 'I' | 'O' | 'U' | 'Y' => 0x80 | 0x40,
        'B' => 0x80 | 0x20 | 0x08,
        'C' => 0x80 | 0x20 | 0x10,
        'D' => 0x80 | 0x20 | 0x08 | 0x04,
        'F' => 0x80 | 0x20,
        'G' => 0x80 | 0x20 | 0x10 | 0x08,
        'H' => 0x80 | 0x20,
        'J' => 0x80 | 0x20 | 0x10 | 0x08 | 0x04,
        'K' => 0x80 | 0x20,
        'L' => 0x80 | 0x20 | 0x08 | 0x04,
        'M' => 0x80 | 0x20 | 0x08,
        'N' => 0x80 | 0x20 | 0x08 | 0x04,
        'P' => 0x80 | 0x20,
        'Q' => 0x80 | 0x20,
        'R' => 0x80 | 0x20 | 0x08 | 0x04,
        'S' => 0x80 | 0x20 | 0x10 | 0x04,
        'T' => 0x80 | 0x20 | 0x04,
        'V' => 0x80 | 0x20 | 0x08,
        'W' => 0x80 | 0x20 | 0x08,
        'X' => 0x80 | 0x20 | 0x10,
        'Z' => 0x80 | 0x20 | 0x10 | 0x08 | 0x04,
        '0'..='9' => 0x02 | 0x01,
        '\'' => 0x80 | 0x02,
        '`' => 0x20,
        ' ' | '(' | ')' | '[' | '\\' | ']' | '_' => 0,
        // The remaining printable punctuation routes through rule set 2.
        '!' | '"' | '#' | '$' | '%' | '&' | '*' | '+' | ',' | '-' | '.' | '/' | ':' | ';'
        | '<' | '=' | '>' | '?' | '@' | '^' => 0x02,
        _ => 0,
    }
}

#[inline]
fn flags(c: char, flg: u8) -> bool {
    (cf(c) & flg) != 0
}

#[inline]
fn char_at(text: &[char], pos: i32) -> Option<char> {
    if pos < 0 {
        None
    } else {
        text.get(pos as usize).copied()
    }
}

#[inline]
fn flags_at(text: &[char], pos: i32, flg: u8) -> bool {
    matches!(char_at(text, pos), Some(c) if flags(c, flg))
}

/// Compare the two characters at `start` against a list of 2-char options.
fn is_one_of_2(text: &[char], start: i32, options: &[[char; 2]]) -> bool {
    let (a, b) = (char_at(text, start), char_at(text, start + 1));
    match (a, b) {
        (Some(a), Some(b)) => options.iter().any(|o| o[0] == a && o[1] == b),
        _ => false,
    }
}

// ── Rule model ───────────────────────────────────────────────────────────────

struct Rule {
    pre: Vec<char>,
    match_: Vec<char>,
    post: Vec<char>,
    target: String,
}

/// Parse `'pre(match)post=target'` into a [`Rule`] (mirrors `reciterRule`).
fn parse_rule(s: &str) -> Rule {
    let parts: Vec<&str> = s.split('=').collect();
    let target = parts[parts.len() - 1].to_string();
    let source = parts[..parts.len() - 1].join("=");

    let src: Vec<&str> = source.split('(').collect();
    let pre: Vec<char> = src[0].chars().collect();
    let last = src[src.len() - 1];
    let mut mp = last.splitn(2, ')');
    let match_: Vec<char> = mp.next().unwrap_or("").chars().collect();
    let post: Vec<char> = mp.next().unwrap_or("").chars().collect();

    Rule { pre, match_, post, target }
}

/// Test if the rule prefix matches, walking left from `pos`.
fn check_prefix(pre: &[char], text: &[char], start: i32) -> bool {
    let mut pos = start;
    for rp in (0..pre.len()).rev() {
        let rule_byte = pre[rp];
        if !flags(rule_byte, FLAG_ALPHA_OR_QUOT) {
            let ok = match rule_byte {
                ' ' => {
                    pos -= 1;
                    !flags_at(text, pos, FLAG_ALPHA_OR_QUOT)
                }
                '#' => {
                    pos -= 1;
                    flags_at(text, pos, FLAG_VOWEL_OR_Y)
                }
                '.' => {
                    pos -= 1;
                    flags_at(text, pos, FLAG_0X08)
                }
                '&' => {
                    pos -= 1;
                    if flags_at(text, pos, FLAG_DIPHTHONG) {
                        true
                    } else {
                        pos -= 1;
                        is_one_of_2(text, pos, &[['C', 'H'], ['S', 'H']])
                    }
                }
                '@' => {
                    pos -= 1;
                    // Reduces to the voiced test: the H/TCS branch is always false.
                    flags_at(text, pos, FLAG_VOICED)
                }
                '^' => {
                    pos -= 1;
                    flags_at(text, pos, FLAG_CONSONANT)
                }
                '+' => {
                    pos -= 1;
                    matches!(char_at(text, pos), Some('E') | Some('I') | Some('Y'))
                }
                ':' => {
                    while pos >= 0 {
                        if !flags_at(text, pos - 1, FLAG_CONSONANT) {
                            break;
                        }
                        pos -= 1;
                    }
                    true
                }
                _ => false,
            };
            if !ok {
                return false;
            }
        } else {
            pos -= 1;
            if char_at(text, pos) != Some(rule_byte) {
                return false;
            }
        }
    }
    true
}

/// Test if the rule suffix matches, walking right from `pos`.
fn check_suffix(post: &[char], text: &[char], start: i32) -> bool {
    let mut pos = start;
    for &rule_byte in post {
        if !flags(rule_byte, FLAG_ALPHA_OR_QUOT) {
            let ok = match rule_byte {
                ' ' => {
                    pos += 1;
                    !flags_at(text, pos, FLAG_ALPHA_OR_QUOT)
                }
                '#' => {
                    pos += 1;
                    flags_at(text, pos, FLAG_VOWEL_OR_Y)
                }
                '.' => {
                    pos += 1;
                    flags_at(text, pos, FLAG_0X08)
                }
                '&' => {
                    pos += 1;
                    if flags_at(text, pos, FLAG_DIPHTHONG) {
                        true
                    } else {
                        pos += 1;
                        is_one_of_2(text, pos - 2, &[['H', 'C'], ['H', 'S']])
                    }
                }
                '@' => {
                    pos += 1;
                    flags_at(text, pos, FLAG_VOICED)
                }
                '^' => {
                    pos += 1;
                    flags_at(text, pos, FLAG_CONSONANT)
                }
                '+' => {
                    pos += 1;
                    matches!(char_at(text, pos), Some('E') | Some('I') | Some('Y'))
                }
                ':' => {
                    while flags_at(text, pos + 1, FLAG_CONSONANT) {
                        pos += 1;
                    }
                    true
                }
                '%' => check_percent(text, &mut pos),
                _ => false,
            };
            if !ok {
                return false;
            }
        } else {
            pos += 1;
            if char_at(text, pos) != Some(rule_byte) {
                return false;
            }
        }
    }
    true
}

/// The `%` suffix matcher: ING / E / ER / ES / ED / EFUL / ELY.
fn check_percent(text: &[char], pos: &mut i32) -> bool {
    let p = *pos;
    if char_at(text, p + 1) != Some('E') {
        if substr(text, p + 1, 3) == "ING" {
            *pos += 3;
            return true;
        }
        return false;
    }
    if !flags_at(text, p + 2, FLAG_ALPHA_OR_QUOT) {
        *pos += 1;
        return true;
    }
    let c2 = char_at(text, p + 2);
    if !matches!(c2, Some('R') | Some('S') | Some('D')) {
        if c2 != Some('L') {
            if substr(text, p + 2, 3) == "FUL" {
                *pos += 4;
                return true;
            }
            return false;
        }
        if char_at(text, p + 3) != Some('Y') {
            return false;
        }
        *pos += 3;
        return true;
    }
    *pos += 2;
    true
}

fn substr(text: &[char], start: i32, len: usize) -> String {
    if start < 0 {
        return String::new();
    }
    let s = start as usize;
    text.iter().skip(s).take(len).collect()
}

fn starts_with(text: &[char], pos: i32, m: &[char]) -> bool {
    if pos < 0 {
        return false;
    }
    let p = pos as usize;
    if p + m.len() > text.len() {
        return false;
    }
    text[p..p + m.len()] == *m
}

fn rule_matches(rule: &Rule, text: &[char], pos: i32) -> bool {
    if !starts_with(text, pos, &rule.match_) {
        return false;
    }
    if !check_prefix(&rule.pre, text, pos) {
        return false;
    }
    check_suffix(&rule.post, text, pos + rule.match_.len() as i32 - 1)
}

// ── Compiled rule set (built once) ───────────────────────────────────────────

struct ReciterRules {
    by_char: HashMap<char, Vec<Rule>>,
    set2: Vec<Rule>,
}

fn rules() -> &'static ReciterRules {
    static RULES: OnceLock<ReciterRules> = OnceLock::new();
    RULES.get_or_init(|| {
        let mut by_char: HashMap<char, Vec<Rule>> = HashMap::new();
        for chunk in RULES_SRC.split('|') {
            let rule = parse_rule(chunk);
            let key = rule.match_[0];
            by_char.entry(key).or_default().push(rule);
        }
        let set2 = RULES2_SRC.split('|').map(parse_rule).collect();
        ReciterRules { by_char, set2 }
    })
}

/// Apply the first matching rule from `group`, appending its target and
/// advancing `input_pos`. Returns whether a rule matched (one always should,
/// thanks to the bare single-letter fallbacks).
fn apply_first(group: &[Rule], text: &[char], pos: i32, input_pos: &mut i32, output: &mut String) -> bool {
    for rule in group {
        if rule_matches(rule, text, pos) {
            output.push_str(&rule.target);
            *input_pos += rule.match_.len() as i32;
            return true;
        }
    }
    false
}

/// Convert English text to a SAM phoneme string. Returns `None` on an
/// unparseable character (matches JS returning `false`).
pub fn text_to_phonemes(input: &str) -> Option<String> {
    let text_s = format!(" {}", input.to_uppercase());
    let text: Vec<char> = text_s.chars().collect();
    let r = rules();

    let mut input_pos = 0i32;
    let mut output = String::new();
    let mut guard = 0;

    while (input_pos as usize) < text.len() && guard < 10000 {
        guard += 1;
        let current = text[input_pos as usize];

        // A '.' not followed by a digit is sentence punctuation.
        if current != '.' || flags_at(&text, input_pos + 1, FLAG_NUMERIC) {
            if flags(current, FLAG_RULESET2) {
                apply_first(&r.set2, &text, input_pos, &mut input_pos, &mut output);
                continue;
            }
            if cf(current) != 0 {
                if !flags(current, FLAG_ALPHA_OR_QUOT) {
                    return None;
                }
                if let Some(group) = r.by_char.get(&current) {
                    apply_first(group, &text, input_pos, &mut input_pos, &mut output);
                } else {
                    // No rules for this char: emit a space and advance to avoid a stall.
                    output.push(' ');
                    input_pos += 1;
                }
                continue;
            }
            output.push(' ');
            input_pos += 1;
            continue;
        }
        output.push('.');
        input_pos += 1;
    }
    Some(output)
}

// ── Rule data (SAM ROM) ──────────────────────────────────────────────────────
// Pipe-separated `pre(match)post=target` rules. Verbatim from sam-js; the bare
// single-letter rules at the end of each group are unconditional fallbacks.

const RULES_SRC: &str = concat!(
    " (A.)=EH4Y. |(A) =AH|", " (ARE) =AAR|", " (AR)O=AXR|(AR)#=EH4R| ^(AS)#=EY4S|",
    "(A)WA=AX|(AW)=AO5| :(ANY)=EH4NIY|(A)^+#=EY5|#:(ALLY)=ULIY| (AL)#=UL|",
    "(AGAIN)=AXGEH4N|#:(AG)E=IHJ|(A)^%=EY|(A)^+:#=AE| :(A)^+ =EY4| (ARR)=AXR|",
    "(ARR)=AE4R| ^(AR) =AA5R|(AR)=AA5R|(AIR)=EH4R|(AI)=EY4|(AY)=EY5|(AU)=AO4|",
    "#:(AL) =UL|#:(ALS) =ULZ|(ALK)=AO4K|(AL)^=AOL| :(ABLE)=EY4BUL|(ABLE)=AXBUL|",
    "(A)VO=EY4|(ANG)+=EY4NJ|(ATARI)=AHTAA4RIY|(A)TOM=AE|(A)TTI=AE| (AT) =AET|",
    " (A)T=AH|(A)=AE|",
    " (B) =BIY4| (BE)^#=BIH|(BEING)=BIY4IHNX| (BOTH) =BOW4TH| (BUS)#=BIH4Z|",
    "(BREAK)=BREY5K|(BUIL)=BIH4L|(B)=B|",
    " (C) =SIY4| (CH)^=K|^E(CH)=K|(CHA)R#=KEH5|(CH)=CH| S(CI)#=SAY4|(CI)A=SH|",
    "(CI)O=SH|(CI)EN=SH|(CITY)=SIHTIY|(C)+=S|(CK)=K|(COMMODORE)=KAA4MAHDOHR|",
    "(COM)=KAHM|(CUIT)=KIHT|(CREA)=KRIYEY|(C)=K|",
    " (D) =DIY4| (DR.) =DAA4KTER|#:(DED) =DIHD|.E(D) =D|#:^E(D) =T| (DE)^#=DIH|",
    " (DO) =DUW| (DOES)=DAHZ|(DONE) =DAH5N|(DOING)=DUW4IHNX| (DOW)=DAW|#(DU)A=JUW|",
    "#(DU)^#=JAX|(D)=D|",
    " (E) =IYIY4|#:(E) =|':^(E) =| :(E) =IY|#(ED) =D|#:(E)D =|(EV)ER=EH4V|(E)^%=IY4|",
    "(ERI)#=IY4RIY|(ERI)=EH4RIH|#:(ER)#=ER|(ERROR)=EH4ROHR|(ERASE)=IHREY5S|(ER)#=EHR|",
    "(ER)=ER| (EVEN)=IYVEHN|#:(E)W=|@(EW)=UW|(EW)=YUW|(E)O=IY|#:&(ES) =IHZ|#:(E)S =|",
    "#:(ELY) =LIY|#:(EMENT)=MEHNT|(EFUL)=FUHL|(EE)=IY4|(EARN)=ER5N| (EAR)^=ER5|",
    "(EAD)=EHD|#:(EA) =IYAX|(EA)SU=EH5|(EA)=IY5|(EIGH)=EY4|(EI)=IY4| (EYE)=AY4|",
    "(EY)=IY|(EU)=YUW5|(EQUAL)=IY4KWUL|(E)=EH|",
    " (F) =EH4F|(FUL)=FUHL|(FRIEND)=FREH5ND|(FATHER)=FAA4DHER|(F)F=|(F)=F|",
    " (G) =JIY4|(GIV)=GIH5V| (G)I^=G|(GE)T=GEH5|SU(GGES)=GJEH4S|(GG)=G| B#(G)=G|",
    "(G)+=J|(GREAT)=GREY4T|(GON)E=GAO5N|#(GH)=| (GN)=N|(G)=G|",
    " (H) =EY4CH| (HAV)=/HAE6V| (HERE)=/HIYR| (HOUR)=AW5ER|(HOW)=/HAW|(H)#=/H|(H)=|",
    " (IN)=IHN| (I) =AY4|(I) =AY|(IN)D=AY5N|SEM(I)=IY| ANT(I)=AY|(IER)=IYER|",
    "#:R(IED) =IYD|(IED) =AY5D|(IEN)=IYEHN|(IE)T=AY4EH|(I')=AY5| :(I)^%=AY5|",
    " :(IE) =AY4|(I)%=IY|(IE)=IY4| (IDEA)=AYDIY5AH|(I)^+:#=IH|(IR)#=AYR|(IZ)%=AYZ|",
    "(IS)%=AYZ|I^(I)^#=IH|+^(I)^+=AY|#:^(I)^+=IH|(I)^+=AY|(IR)=ER|(IGH)=AY4|",
    "(ILD)=AY5LD| (IGN)=IHGN|(IGN) =AY4N|(IGN)^=AY4N|(IGN)%=AY4N|(ICRO)=AY4KROH|",
    "(IQUE)=IY4K|(I)=IH|",
    " (J) =JEY4|(J)=J|",
    " (K) =KEY4| (K)N=|(K)=K|",
    " (L) =EH4L|(LO)C#=LOW|L(L)=|#:^(L)%=UL|(LEAD)=LIYD| (LAUGH)=LAE4F|(L)=L|",
    " (M) =EH4M| (MR.) =MIH4STER| (MS.)=MIH5Z| (MRS.) =MIH4SIXZ|(MOV)=MUW4V|",
    "(MACHIN)=MAHSHIY5N|M(M)=|(M)=M|",
    " (N) =EH4N|E(NG)+=NJ|(NG)R=NXG|(NG)#=NXG|(NGL)%=NXGUL|(NG)=NX|(NK)=NXK|",
    " (NOW) =NAW4|N(N)=|(NON)E=NAH4N|(N)=N|",
    " (O) =OH4W|(OF) =AHV| (OH) =OW5|(OROUGH)=ER4OW|#:(OR) =ER|#:(ORS) =ERZ|(OR)=AOR|",
    " (ONE)=WAHN|#(ONE) =WAHN|(OW)=OW| (OVER)=OW5VER|PR(O)V=UW4|(OV)=AH4V|(O)^%=OW5|",
    "(O)^EN=OW|(O)^I#=OW5|(OL)D=OW4L|(OUGHT)=AO5T|(OUGH)=AH5F| (OU)=AW|H(OU)S#=AW4|",
    "(OUS)=AXS|(OUR)=OHR|(OULD)=UH5D|(OU)^L=AH5|(OUP)=UW5P|(OU)=AW|(OY)=OY|",
    "(OING)=OW4IHNX|(OI)=OY5|(OOR)=OH5R|(OOK)=UH5K|F(OOD)=UW5D|L(OOD)=AH5D|",
    "M(OOD)=UW5D|(OOD)=UH5D|F(OOT)=UH5T|(OO)=UW5|(O')=OH|(O)E=OW|(O) =OW|(OA)=OW4|",
    " (ONLY)=OW4NLIY| (ONCE)=WAH4NS|(ON'T)=OW4NT|C(O)N=AA|(O)NG=AO| :^(O)N=AH|",
    "I(ON)=UN|#:(ON)=UN|#^(ON)=UN|(O)ST=OW|(OF)^=AO4F|(OTHER)=AH5DHER|R(O)B=RAA|",
    "^R(O):#=OW5|(OSS) =AO5S|#:^(OM)=AHM|(O)=AA|",
    " (P) =PIY4|(PH)=F|(PEOPL)=PIY5PUL|(POW)=PAW4|(PUT) =PUHT|(P)P=|(P)S=|(P)N=|",
    "(PROF.)=PROHFEH4SER|(P)=P|",
    " (Q) =KYUW4|(QUAR)=KWOH5R|(QU)=KW|(Q)=K|",
    " (R) =AA5R| (RE)^#=RIY|(R)R=|(R)=R|",
    " (S) =EH4S|(SH)=SH|#(SION)=ZHUN|(SOME)=SAHM|#(SUR)#=ZHER|(SUR)#=SHER|#(SU)#=ZHUW|",
    "#(SSU)#=SHUW|#(SED)=ZD|#(S)#=Z|(SAID)=SEHD|^(SION)=SHUN|(S)S=|.(S) =Z|#:.E(S) =Z|",
    "#:^#(S) =S|U(S) =S| :#(S) =Z|##(S) =Z| (SCH)=SK|(S)C+=|#(SM)=ZUM|#(SN)'=ZUM|",
    "(STLE)=SUL|(S)=S|",
    " (T) =TIY4| (THE) #=DHIY| (THE) =DHAX|(TO) =TUX| (THAT)=DHAET| (THIS) =DHIHS|",
    " (THEY)=DHEY| (THERE)=DHEHR|(THER)=DHER|(THEIR)=DHEHR| (THAN) =DHAEN|",
    " (THEM) =DHAEN|(THESE) =DHIYZ| (THEN)=DHEHN|(THROUGH)=THRUW4|(THOSE)=DHOHZ|",
    "(THOUGH) =DHOW|(TODAY)=TUXDEY|(TOMO)RROW=TUMAA5|(TO)TAL=TOW5| (THUS)=DHAH4S|",
    "(TH)=TH|#:(TED)=TIXD|S(TI)#N=CH|(TI)O=SH|(TI)A=SH|(TIEN)=SHUN|(TUR)#=CHER|",
    "(TU)A=CHUW| (TWO)=TUW|&(T)EN =|(T)=T|",
    " (U) =YUW4| (UN)I=YUWN| (UN)=AHN| (UPON)=AXPAON|@(UR)#=UH4R|(UR)#=YUH4R|(UR)=ER|",
    "(U)^ =AH|(U)^^=AH5|(UY)=AY5| G(U)#=|G(U)%=|G(U)#=W|#N(U)=YUW|@(U)=UW|(U)=YUW|",
    " (V) =VIY4|(VIEW)=VYUW5|(V)=V|",
    " (W) =DAH4BULYUW| (WERE)=WER|(WA)SH=WAA|(WA)ST=WEY|(WA)S=WAH|(WA)T=WAA|",
    "(WHERE)=WHEHR|(WHAT)=WHAHT|(WHOL)=/HOWL|(WHO)=/HUW|(WH)=WH|(WAR)#=WEHR|",
    "(WAR)=WAOR|(WOR)^=WER|(WR)=R|(WOM)A=WUHM|(WOM)E=WIHM|(WEA)R=WEH|(WANT)=WAA5NT|",
    "ANS(WER)=ER|(W)=W|",
    " (X) =EH4KR| (X)=Z|(X)=KS|",
    " (Y) =WAY4|(YOUNG)=YAHNX| (YOUR)=YOHR| (YOU)=YUW| (YES)=YEHS| (Y)=Y|F(Y)=AY|",
    "PS(YCH)=AYK|#:^(Y)=IY|#:^(Y)I=IY| :(Y) =AY| :(Y)#=AY| :(Y)^+:#=IH| :(Y)^#=AY|",
    "(Y)=IH|",
    " (Z) =ZIY4|(Z)=Z",
);

const RULES2_SRC: &str = concat!(
    "(A)=|(!)=.|(\") =-AH5NKWOWT-|(\")=KWOW4T-|(#)= NAH4MBER|($)= DAA4LER|",
    "(%)= PERSEH4NT|(&)= AEND|(')=|(*)= AE4STERIHSK|(+)= PLAH4S|(,)=,| (-) =-|(-)=|",
    "(.)= POYNT|(/)= SLAE4SH|(0)= ZIY4ROW| (1ST)=FER4ST| (10TH)=TEH4NTH|(1)= WAH4N|",
    " (2ND)=SEH4KUND|(2)= TUW4| (3RD)=THER4D|(3)= THRIY4|(4)= FOH4R| (5TH)=FIH4FTH|",
    "(5)= FAY4V| (64) =SIH4KSTIY FOHR|(6)= SIH4KS|(7)= SEH4VUN| (8TH)=EY4TH|",
    "(8)= EY4T|(9)= NAY4N|(:)=.|(;)=.|(<)= LEH4S DHAEN|(=)= IY4KWULZ|",
    "(>)= GREY4TER DHAEN|(?)=?|(@)= AE6T|(^)= KAE4RIXT",
);
