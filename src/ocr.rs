//! Reads the text in a picture with the `tesseract` program, keeping where each word is
//! so the editor can let the user select words on the screenshot.

use std::io::Write;
use std::process::{Command, Stdio};

use image::RgbaImage;

use crate::output;

/// One recognized word; the box is in the picture's pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub text: String,
    /// Words with the same line number sit on one line of one paragraph.
    pub line: usize,
    pub paragraph: usize,
    /// How sure tesseract was, 0 to 100.
    pub confidence: f32,
}

/// Anything below this is noise tesseract invented out of icons and gradients.
const MIN_CONFIDENCE: f32 = 50.0;
const SHORT_WORD_CONFIDENCE: f32 = 85.0;

/// Recognizes the words in `image`, in reading order. `languages` is tesseract's `-l`
/// value (`por+eng`); empty picks the languages that suit the system.
pub fn recognize(image: &RgbaImage, languages: &str) -> Result<Vec<Word>, String> {
    let languages = if languages.trim().is_empty() { auto_languages() } else { languages.trim().to_string() };
    let mut words = read(image, &languages)?;
    // Small text (labels, tables) is read better, and differently, when enlarged: read it again
    // at twice the size and keep the surer reading of each word.
    let height = {
        let mut heights: Vec<f32> = words.iter().map(|w| w.h).collect();
        heights.sort_by(f32::total_cmp);
        heights.get(heights.len() / 2).copied().unwrap_or(0.0)
    };
    let big = image.width().max(image.height()) > 4000;
    if !words.is_empty() && height < ENLARGE_BELOW && !big {
        let (w, h) = (image.width() * 2, image.height() * 2);
        let larger = image::imageops::resize(image, w, h, image::imageops::FilterType::Lanczos3);
        let again = read(&larger, &languages)?.into_iter().map(|word| Word {
            x: word.x / 2.0,
            y: word.y / 2.0,
            w: word.w / 2.0,
            h: word.h / 2.0,
            ..word
        });
        words = merge(words, again.collect());
    }
    Ok(arrange(words))
}

/// Anything shorter than this (median word height, in pixels) is read a second time, enlarged.
const ENLARGE_BELOW: f32 = 30.0;

/// The words of `first`, except where a word of `second` over the same place is surer.
fn merge(mut first: Vec<Word>, second: Vec<Word>) -> Vec<Word> {
    let centre = |w: &Word| (w.x + w.w / 2.0, w.y + w.h / 2.0);
    let covers = |a: &Word, b: &Word| {
        let (cx, cy) = centre(b);
        cx >= a.x && cx <= a.x + a.w && cy >= a.y && cy <= a.y + a.h
    };
    for word in second {
        let over: Vec<usize> =
            (0..first.len()).filter(|&i| covers(&first[i], &word) || covers(&word, &first[i])).collect();
        let surer = over.iter().all(|&i| word.confidence > first[i].confidence + 5.0);
        if over.is_empty() || surer {
            for &i in over.iter().rev() {
                first.remove(i);
            }
            first.push(word);
        }
    }
    first
}

/// One tesseract run; the words are in `image`'s pixels.
fn read(image: &RgbaImage, languages: &str) -> Result<Vec<Word>, String> {
    let png = output::encode_png_fast(image)?;
    let mut command = Command::new("tesseract");
    // One thread: it is as fast on a selection and doesn't spike every core.
    // Sparse text (11) finds words anywhere, which is what a screenshot has: buttons, labels, tables.
    command.env("OMP_THREAD_LIMIT", "1").args(["stdin", "stdout", "--psm", "11"]);
    if !languages.is_empty() {
        command.args(["-l", languages]);
    }
    command.arg("tsv");
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "missing".to_string() } else { e.to_string() })?;
    // The picture can be larger than the pipe, so it is written while the output is read.
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let writer = std::thread::spawn(move || stdin.write_all(&png));
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let _ = writer.join();
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(why.lines().last().unwrap_or("tesseract failed").trim().to_string());
    }
    Ok(parse_tsv(&String::from_utf8_lossy(&out.stdout)))
}

/// The language of the session plus English, as far as they are installed; with none of
/// them installed, whatever is, so something is read.
fn auto_languages() -> String {
    let installed = installed_languages();
    pick_languages(&installed, &session_languages())
}

fn session_languages() -> Vec<String> {
    ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"]
        .iter()
        .filter_map(|var| std::env::var(var).ok())
        .flat_map(|value| value.split(':').map(str::to_string).collect::<Vec<_>>())
        .filter_map(|locale| tesseract_code(locale.split(['_', '-', '.', '@']).next()?).map(str::to_string))
        .collect()
}

fn installed_languages() -> Vec<String> {
    let Ok(out) = Command::new("tesseract").arg("--list-langs").output() else { return Vec::new() };
    // The list goes to stdout on 4.x and to stderr on some older ones.
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    text.lines().skip_while(|l| !l.starts_with("List of")).skip(1).map(|l| l.trim().to_string()).collect()
}

fn pick_languages(installed: &[String], wanted: &[String]) -> String {
    let has = |code: &str| installed.iter().any(|i| i == code);
    let mut chosen: Vec<&str> = Vec::new();
    for code in wanted.iter().map(String::as_str).chain(["eng"]) {
        if has(code) && !chosen.contains(&code) {
            chosen.push(code);
        }
    }
    if chosen.is_empty() {
        chosen = installed.iter().map(String::as_str).filter(|l| *l != "osd").take(3).collect();
    }
    chosen.join("+")
}

/// Tesseract names languages by three letters; these are the usual two-letter codes.
fn tesseract_code(two: &str) -> Option<&'static str> {
    Some(match two.to_ascii_lowercase().as_str() {
        "en" => "eng",
        "pt" => "por",
        "es" => "spa",
        "fr" => "fra",
        "de" => "deu",
        "it" => "ita",
        "nl" => "nld",
        "ru" => "rus",
        "pl" => "pol",
        "tr" => "tur",
        "sv" => "swe",
        "ja" => "jpn",
        "ko" => "kor",
        "zh" => "chi_sim",
        "ar" => "ara",
        "hi" => "hin",
        "uk" => "ukr",
        "cs" => "ces",
        _ => return None,
    })
}

/// Words from tesseract's TSV output (level 5 rows).
fn parse_tsv(tsv: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let (mut line_key, mut para_key) = (None, None);
    let (mut line, mut paragraph) = (0, 0);
    for row in tsv.lines().skip(1) {
        let f: Vec<&str> = row.splitn(12, '\t').collect();
        if f.len() < 12 || f[0] != "5" {
            continue;
        }
        let text = f[11].trim();
        let num = |i: usize| f[i].parse::<f32>().ok();
        let (Some(x), Some(y), Some(w), Some(h), Some(conf)) = (num(6), num(7), num(8), num(9), num(10)) else {
            continue;
        };
        // Borders and icons come out as one or two doubtful letters (`at`, `J`); real short
        // words (`UX`, `to`) are read with confidence.
        let doubtful = text.chars().count() <= 2 && conf < SHORT_WORD_CONFIDENCE;
        if text.is_empty() || conf < MIN_CONFIDENCE || doubtful || !looks_like_text(text) {
            continue;
        }
        let para = (f[2], f[3]);
        let this_line = (f[2], f[3], f[4]);
        if para_key != Some(para) {
            para_key = Some(para);
            paragraph += 1;
        }
        if line_key != Some(this_line) {
            line_key = Some(this_line);
            line += 1;
        }
        words.push(Word { x, y, w, h, text: text.to_string(), line, paragraph, confidence: conf });
    }
    words
}

/// Icons and borders come out as `|`, `O_O` and the like: at least half of a word must be letters or digits.
fn looks_like_text(text: &str) -> bool {
    let total = text.chars().count();
    let real = text.chars().filter(|c| c.is_alphanumeric()).count();
    real > 0 && real * 2 >= total && !text.contains('_')
}

/// Puts the words in reading order by where they are, ignoring the blocks tesseract made
/// (sparse recognition returns them cell by cell). Words side by side with a space between them,
/// or one right under another, belong together: that makes a block, which is a paragraph or a
/// label of several lines. Blocks are read row by row, left to right.
fn arrange(words: Vec<Word>) -> Vec<Word> {
    let mut group: Vec<usize> = (0..words.len()).collect();
    fn root(group: &mut [usize], mut i: usize) -> usize {
        while group[i] != i {
            group[i] = group[group[i]];
            i = group[i];
        }
        i
    }
    for (i, a) in words.iter().enumerate() {
        for (j, b) in words.iter().enumerate().skip(i + 1) {
            let h = a.h.min(b.h);
            let sideways = a.x.max(b.x) - (a.x + a.w).min(b.x + b.w); // negative: they overlap
            let vertical = a.y.max(b.y) - (a.y + a.h).min(b.y + b.h); // negative: they overlap
            let beside = vertical < -0.5 * h && sideways < h;
            let stacked = vertical > -0.3 * h && vertical < h && sideways < -0.3 * a.w.min(b.w);
            if beside || stacked {
                let (ri, rj) = (root(&mut group, i), root(&mut group, j));
                group[ri] = rj;
            }
        }
    }
    let mut blocks: Vec<Vec<Word>> = Vec::new();
    let mut slot: Vec<Option<usize>> = vec![None; words.len()];
    for (i, word) in words.into_iter().enumerate() {
        let r = root(&mut group, i);
        let at = *slot[r].get_or_insert_with(|| {
            blocks.push(Vec::new());
            blocks.len() - 1
        });
        blocks[at].push(word);
    }

    // Lines inside a block: words whose middles are at about the same height.
    let mid = |w: &Word| w.y + w.h / 2.0;
    let blocks: Vec<Vec<Vec<Word>>> = blocks
        .into_iter()
        .map(|mut block| {
            block.sort_by(|a, b| mid(a).total_cmp(&mid(b)));
            let mut lines: Vec<Vec<Word>> = Vec::new();
            for word in block {
                let fits = lines.last().is_some_and(|line| (mid(&word) - mid(&line[0])).abs() <= 0.6 * word.h);
                match lines.last_mut() {
                    Some(line) if fits => line.push(word),
                    _ => lines.push(vec![word]),
                }
            }
            for line in &mut lines {
                line.sort_by(|a, b| a.x.total_cmp(&b.x));
            }
            lines
        })
        .collect();

    // Rows of blocks: the ones whose middles sit at about the same height.
    let bounds = |b: &Vec<Vec<Word>>| {
        let all = || b.iter().flatten();
        let top = all().map(|w| w.y).fold(f32::MAX, f32::min);
        let bottom = all().map(|w| w.y + w.h).fold(f32::MIN, f32::max);
        let left = all().map(|w| w.x).fold(f32::MAX, f32::min);
        ((top + bottom) / 2.0, bottom - top, left)
    };
    let mut blocks = blocks;
    blocks.sort_by(|a, b| bounds(a).0.total_cmp(&bounds(b).0));
    let mut rows: Vec<Vec<Vec<Vec<Word>>>> = Vec::new();
    let mut row_mid = 0.0;
    for block in blocks {
        let (centre, height, _) = bounds(&block);
        if !rows.is_empty() && (centre - row_mid).abs() <= 0.25 * height {
            rows.last_mut().unwrap().push(block);
        } else {
            row_mid = centre;
            rows.push(vec![block]);
        }
    }

    let (mut out, mut line, mut paragraph) = (Vec::new(), 0, 0);
    for mut row in rows {
        row.sort_by(|a, b| bounds(a).2.total_cmp(&bounds(b).2));
        for block in row {
            paragraph += 1;
            for words in block {
                line += 1;
                for mut word in words {
                    word.line = line;
                    word.paragraph = paragraph;
                    out.push(word);
                }
            }
        }
    }
    out
}

/// The text of `words[from..=to]`: spaces inside a line, a new line between lines and
/// a blank one between paragraphs.
pub fn join(words: &[Word]) -> String {
    let mut out = String::new();
    for (i, word) in words.iter().enumerate() {
        if let Some(before) = i.checked_sub(1).map(|j| &words[j]) {
            out.push_str(if word.paragraph != before.paragraph {
                "\n\n"
            } else if word.line != before.line {
                "\n"
            } else {
                " "
            });
        }
        out.push_str(&word.text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs tesseract and QUEIJO_OCR_IMAGE, a picture with text"]
    fn reads_a_real_picture() {
        let path = std::env::var("QUEIJO_OCR_IMAGE").expect("QUEIJO_OCR_IMAGE");
        let image = image::open(path).unwrap().to_rgba8();
        let words = recognize(&image, "").unwrap();
        println!("{}", join(&words));
        assert!(words.len() > 5);
    }

    const TSV: &str = "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n\
1\t1\t0\t0\t0\t0\t0\t0\t300\t100\t-1\t\n\
4\t1\t1\t1\t1\t0\t10\t10\t120\t20\t-1\t\n\
5\t1\t1\t1\t1\t1\t10\t10\t50\t20\t96.5\tOlá\n\
5\t1\t1\t1\t1\t2\t70\t10\t60\t20\t91\tmundo\n\
5\t1\t1\t1\t2\t1\t10\t40\t40\t20\t88\tsegunda\n\
5\t1\t1\t1\t2\t2\t60\t40\t10\t20\t12\t#\n\
5\t1\t1\t1\t2\t3\t80\t40\t10\t20\t90\t \n\
5\t1\t2\t1\t1\t1\t10\t80\t30\t20\t95\tfim\n";

    #[test]
    fn reads_words_with_their_boxes_and_drops_noise() {
        let words = parse_tsv(TSV);
        let text: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(text, ["Olá", "mundo", "segunda", "fim"], "low confidence and blank words are dropped");
        assert_eq!((words[0].x, words[0].y, words[0].w, words[0].h), (10.0, 10.0, 50.0, 20.0));
    }

    #[test]
    fn joins_words_by_line_and_paragraph() {
        assert_eq!(join(&parse_tsv(TSV)), "Olá mundo\nsegunda\n\nfim");
        assert_eq!(join(&parse_tsv(TSV)[1..3]), "mundo\nsegunda");
        assert_eq!(join(&[]), "");
    }

    fn word(text: &str, x: f32, y: f32) -> Word {
        Word { x, y, w: 10.0 * text.len() as f32, h: 20.0, text: text.into(), line: 0, paragraph: 0, confidence: 90.0 }
    }

    #[test]
    fn arranges_words_by_position_whatever_order_they_came_in() {
        // Cell by cell, as sparse recognition returns a table: two labels side by side, the
        // second one a line shorter and centred on the first.
        let cells = vec![
            word("Product", 10.0, 100.0),
            word("Definition", 10.0, 124.0),
            word("Engineering", 300.0, 112.0),
            word("Complete", 300.0, 136.0),
            word("Title", 10.0, 10.0),
            word("here", 70.0, 10.0),
        ];
        assert_eq!(join(&arrange(cells)), "Title here\n\nProduct\nDefinition\n\nEngineering\nComplete");
    }

    #[test]
    fn a_wrapped_paragraph_stays_together_and_far_blocks_do_not() {
        let words = vec![
            word("first", 10.0, 10.0),
            word("line", 70.0, 10.0),
            word("second", 10.0, 34.0),
            word("line", 80.0, 34.0),
            word("far", 10.0, 200.0),
        ];
        assert_eq!(join(&arrange(words)), "first line\nsecond line\n\nfar");
    }

    #[test]
    fn merging_keeps_the_surer_reading_and_adds_what_was_missed() {
        let mut bad = word("1123)", 100.0, 10.0);
        bad.confidence = 67.0;
        let kept = word("Quality", 10.0, 10.0);
        let mut good = word("11/23)", 102.0, 11.0);
        good.confidence = 94.0;
        let mut unsure = word("Qualitv", 11.0, 11.0);
        unsure.confidence = 88.0;
        let missed = word("Launch", 10.0, 80.0);
        let merged = merge(vec![bad, kept], vec![good, unsure, missed]);
        let text: Vec<&str> = merged.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(text, ["Quality", "11/23)", "Launch"]);
    }

    #[test]
    fn noise_from_icons_is_not_text() {
        for noise in ["|", "O_O", "}", ".—\\Y!O", "—"] {
            assert!(!looks_like_text(noise), "{noise}");
        }
        for text in ["A", "(Completed)", "11/23)", "OSmosis", "10/17)"] {
            assert!(looks_like_text(text), "{text}");
        }
    }

    #[test]
    fn a_tab_inside_the_text_is_kept() {
        let row = "5\t1\t1\t1\t1\t1\t0\t0\t5\t5\t90\ta\tb";
        assert_eq!(parse_tsv(&format!("header\n{row}"))[0].text, "a\tb");
    }

    #[test]
    fn picks_the_session_language_then_english() {
        let installed: Vec<String> = ["eng", "osd", "por", "spa"].map(String::from).into();
        assert_eq!(pick_languages(&installed, &["por".into(), "fra".into()]), "por+eng");
        assert_eq!(pick_languages(&installed, &["eng".into()]), "eng");
        assert_eq!(pick_languages(&installed, &[]), "eng");
        let only_spanish: Vec<String> = ["osd", "spa"].map(String::from).into();
        assert_eq!(pick_languages(&only_spanish, &["por".into()]), "spa");
        assert_eq!(pick_languages(&[], &["por".into()]), "");
    }

    #[test]
    fn maps_locales_to_tesseract_languages() {
        assert_eq!(tesseract_code("pt"), Some("por"));
        assert_eq!(tesseract_code("EN"), Some("eng"));
        assert_eq!(tesseract_code("xx"), None);
    }
}
