//! Parser for winget's fixed-width tables.
//!
//! OWNER: package-managers agent.
//!
//! winget pads every cell to its column width, measured in display columns
//! (one or two per grapheme cluster), separates columns with a space and puts
//! a line of dashes under the header. Column starts are the header positions
//! where a non-space follows a space, or the dash groups when the separator
//! line has several (PowerShell `Format-Table`, used by Scoop).
//!
//! A header made of several words (Korean `사용 가능`, English `Pin type`)
//! yields a false column start; it is dropped when most rows run across it, or
//! when the column it opens is empty in every row.

use unicode_width::UnicodeWidthChar;

use crate::exec::visible_text;

/// One table: header cells (localized, informative only) and rows of cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Finds and parses every table in a winget output (a header line followed by
/// a line of dashes, then rows until a blank or non-row line).
///
/// Must handle: progress/spinner garbage before the table (`\r` overwrites),
/// CRLF, localized headers, column positions computed in *display width*
/// (East-Asian wide chars count 2), cells containing spaces, empty trailing
/// cells, and several tables in one output.
pub fn parse_tables(output: &str) -> Vec<Table> {
    let output = output.strip_prefix('\u{feff}').unwrap_or(output);
    let lines: Vec<Line> = output.split('\n').map(Line::new).collect();
    let mut tables = Vec::new();
    let mut i = 1;
    while i < lines.len() {
        if !starts_table(&lines, i) {
            i += 1;
            continue;
        }
        let header = &lines[i - 1];
        let (mut starts, from_header) = match lines[i].dash_groups() {
            Some(groups) => (groups, false),
            None => (header.word_starts(), true),
        };

        let mut end = i + 1;
        while end < lines.len()
            && !lines[end].is_blank()
            && !starts_table(&lines, end + 1)
            && lines[end].is_row(&starts)
        {
            end += 1;
        }
        let body = &lines[i + 1..end];

        if from_header {
            drop_crossed_starts(&mut starts, body);
        }
        let mut rows = cells_of(body, &starts);
        if from_header && drop_empty_columns(&mut starts, &rows) {
            rows = cells_of(body, &starts);
        }

        tables.push(Table { headers: header.cells(&starts), rows });
        i = end;
    }
    tables
}

/// Is `lines[i]` the dash line under a header?
fn starts_table(lines: &[Line], i: usize) -> bool {
    i >= 1 && i < lines.len() && lines[i].is_separator() && !lines[i - 1].is_blank() && !lines[i - 1].is_separator()
}

fn cells_of(body: &[Line], starts: &[usize]) -> Vec<Vec<String>> {
    body.iter().map(|line| line.cells(starts)).collect()
}

/// Removes the column starts that most rows having text there run across.
fn drop_crossed_starts(starts: &mut Vec<usize>, body: &[Line]) {
    let mut k = 0;
    starts.retain(|&col| {
        k += 1;
        if k == 1 {
            return true;
        }
        let with_text = body.iter().filter(|line| line.inked(col)).count();
        let crossing = body.iter().filter(|line| line.inked(col) && line.inked(col - 1)).count();
        crossing * 2 <= with_text
    });
}

/// Removes the columns (except the first) that are empty in every row.
/// Returns whether anything changed.
fn drop_empty_columns(starts: &mut Vec<usize>, rows: &[Vec<String>]) -> bool {
    if rows.is_empty() {
        return false;
    }
    let before = starts.len();
    let mut k = 0;
    starts.retain(|_| {
        k += 1;
        k == 1 || rows.iter().any(|row| row.get(k - 1).is_some_and(|cell| !cell.is_empty()))
    });
    starts.len() != before
}

/// One physical output line as a terminal displays it.
struct Line {
    /// Visible characters with the display column where each one starts.
    chars: Vec<(usize, char)>,
    /// One entry per display column: covered by a non-blank character.
    ink: Vec<bool>,
}

impl Line {
    fn new(raw: &str) -> Line {
        let text = visible_text(raw.trim_end_matches('\r'));
        let text = text.trim_end();
        let mut chars = Vec::with_capacity(text.len());
        let mut ink = Vec::with_capacity(text.len());
        let mut measure = Measure::default();
        for c in text.chars() {
            match measure.width(c) {
                // Extends the previous grapheme cluster: same column.
                0 => chars.push((chars.last().map_or(0, |&(col, _)| col), c)),
                width => {
                    chars.push((ink.len(), c));
                    ink.extend(std::iter::repeat_n(!c.is_whitespace(), width));
                }
            }
        }
        Line { chars, ink }
    }

    fn is_blank(&self) -> bool {
        !self.ink.contains(&true)
    }

    fn inked(&self, col: usize) -> bool {
        self.ink.get(col).copied().unwrap_or(false)
    }

    /// Dashes and spaces only, starting with a dash.
    fn is_separator(&self) -> bool {
        self.chars.first().is_some_and(|&(_, c)| c == '-')
            && self.chars.iter().all(|&(_, c)| c == '-' || c == ' ')
            && self.chars.iter().filter(|&&(_, c)| c == '-').count() >= 2
    }

    /// Starts of the dash groups, when the separator line has several.
    fn dash_groups(&self) -> Option<Vec<usize>> {
        let starts = self.word_starts();
        (starts.len() > 1).then_some(starts)
    }

    /// Display columns where a word starts; the first one is always 0.
    fn word_starts(&self) -> Vec<usize> {
        let mut starts: Vec<usize> =
            (0..self.ink.len()).filter(|&col| self.ink[col] && (col == 0 || !self.ink[col - 1])).collect();
        match starts.first_mut() {
            Some(first) => *first = 0,
            None => starts.push(0),
        }
        starts
    }

    /// Text between display columns `from` (included) and `to` (excluded).
    fn slice(&self, from: usize, to: usize) -> String {
        let text: String = self.chars.iter().filter(|&&(col, _)| col >= from && col < to).map(|&(_, c)| c).collect();
        text.trim().to_string()
    }

    /// Could this be a row of a table whose columns start at `starts`? Rows
    /// have a second cell (an id, a version, a URL) that is not empty and
    /// holds no space (except pseudo ids such as `ARP\Machine\X64\Some App`),
    /// and no word runs across the start of the second or third column
    /// (headers of the first three columns are single words). Footers are
    /// sentences.
    fn is_row(&self, starts: &[usize]) -> bool {
        if self.is_blank() {
            return false;
        }
        let Some(&second) = starts.get(1) else {
            return true;
        };
        let third = starts.get(2).copied();
        let crosses = |col: usize| col > 0 && self.inked(col - 1) && self.inked(col);
        if crosses(second) || third.is_some_and(crosses) {
            return false;
        }
        let cell = self.slice(second, third.unwrap_or(usize::MAX));
        !cell.is_empty() && (!cell.contains(char::is_whitespace) || cell.contains('\\'))
    }

    /// Cuts the line at the column starts; the last cell takes the rest.
    fn cells(&self, starts: &[usize]) -> Vec<String> {
        let mut cells = vec![String::new(); starts.len()];
        for &(col, c) in &self.chars {
            let k = starts.partition_point(|&start| start <= col).saturating_sub(1);
            if let Some(cell) = cells.get_mut(k) {
                cell.push(c);
            }
        }
        cells.iter().map(|cell| cell.trim().to_string()).collect()
    }
}

/// Display width as winget computes it: per grapheme cluster, two columns when
/// its first character is East-Asian wide or fullwidth, else one. Characters
/// extending a cluster (combining marks, variation selectors, emoji modifiers,
/// emoji joined by ZWJ, second letter of a flag) add nothing.
#[derive(Default)]
struct Measure {
    after_joiner: bool,
    open_flag: bool,
}

impl Measure {
    fn width(&mut self, c: char) -> usize {
        let joined = std::mem::replace(&mut self.after_joiner, c == '\u{200d}');
        let regional = ('\u{1f1e6}'..='\u{1f1ff}').contains(&c);
        let flag_end = regional && self.open_flag;
        self.open_flag = regional && !self.open_flag;
        if joined || flag_end {
            return 0;
        }
        match c {
            // Emoji modifiers, variation selectors, tags.
            '\u{1f3fb}'..='\u{1f3ff}' | '\u{fe00}'..='\u{fe0f}' | '\u{e0000}'..='\u{e0fff}' => 0,
            // First letter of a flag; invisible format characters that form a
            // cluster of their own.
            '\u{1f1e6}'..='\u{1f1ff}'
            | '\u{ad}'
            | '\u{61c}'
            | '\u{200b}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}' => 1,
            _ => c.width().unwrap_or(0),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Joins fixture lines the way winget writes them (CRLF).
    pub(crate) fn crlf(lines: &[&str]) -> String {
        let mut text = lines.join("\r\n");
        text.push_str("\r\n");
        text
    }

    pub(crate) const UPGRADE_FR: &[&str] = &[
        "Nom                               ID                  Version Disponible Source",
        "--------------------------------------------------------------------------------",
        "Git                               Git.Git             2.45.1  2.46.0     winget",
        "Notepad++ (64-bit x64)            Notepad++.Notepad++ 8.6.4   8.6.9      winget",
        "Contoso Sync (Utilisateur actuel) Contoso.Sync        Unknown 2.0.2      winget",
        "Fabrikam Reader                   Fabrikam.Reader     < 3.2.0 3.4.1      winget",
        "Terminal Windows                  9N0DX20HK701        1.20.1  1.21.2     msstore",
        "5 mises à niveau disponibles.",
        "1 package(s) a(ont) des numéros de version qui ne peuvent pas être déterminés. Utilisez --include-unknown pour afficher tous les résultats.",
    ];

    /// English output with spinner garbage, a pinned-count footer and the
    /// "require explicit targeting" second table.
    pub(crate) const UPGRADE_EN_EXPLICIT: &[&str] = &[
        "\r   - \r   \\ \r   | \r   / \r                    \r  ██████████████████████████████  2.50 MB / 2.50 MB",
        "-\u{8}\\\u{8}|\u{8}/\u{8} \u{8}\r                    \rName                        Id                  Version Available Source",
        "------------------------------------------------------------------------",
        "7-Zip 23.01 (x64)           7zip.7zip           23.01   24.08     winget",
        "Mozilla Firefox (x64 en-US) Mozilla.Firefox     128.0   129.0.1   winget",
        "PowerToys (Preview)         Microsoft.PowerToys 0.82.1  0.83.0    winget",
        "3 upgrades available.",
        "1 package(s) have pins that prevent upgrade. Use the 'winget pin' command to view and edit pins. Using the --include-pinned argument may show more results.",
        "",
        "The following packages have an upgrade available, but require explicit targeting for upgrade:",
        "Name                            Id                            Version       Available     Source",
        "------------------------------------------------------------------------------------------------",
        "Microsoft Edge WebView2 Runtime Microsoft.EdgeWebView2Runtime 127.0.2651.74 127.0.2651.86 winget",
    ];

    /// `winget list`: 4 columns, the 4th is Source; pseudo ids; empty cells;
    /// trailing spaces; a `·` (2 bytes, 1 column).
    pub(crate) const LIST_FR_SOURCE: &[&str] = &[
        "Nom                  ID                                                              Version     Source ",
        "--------------------------------------------------------------------------------------------------------",
        "Accueil · Exemple    ARP\\User\\X64\\0123456789abcdef0123456789abcdef                   1.0                ",
        "Café Crème Studio    Contoso.CafeCreme                                               3.1.4       winget ",
        "Calculatrice Windows MSIX\\Microsoft.WindowsCalculator_11.2405.2.0_x64__8wekyb3d8bbwe 11.2405.2.0        ",
        "Fabrikam Backup      ARP\\Machine\\X64\\{01234567-89AB-CDEF-0123-456789ABCDEF}          > 4.2              ",
        "Paint Studio         9PCFS5B6T72H                                                    5.0.13      msstore",
        "Unknown Tool         Contoso.UnknownTool                                             Unknown     winget ",
    ];

    /// `winget list --source winget`: 4 columns, the 4th is Available.
    pub(crate) const LIST_EN_AVAILABLE: &[&str] = &[
        "Name                   Id                 Version Available",
        "-----------------------------------------------------------",
        "Git                    Git.Git            2.45.1  2.46.0",
        "Node.js LTS            OpenJS.NodeJS.LTS  20.15.0",
        "Python 3.12.4 (64-bit) Python.Python.3.12 3.12.4  3.12.5",
    ];

    pub(crate) const LIST_FR_5: &[&str] = &[
        "Nom                ID                                                       Version      Disponible Source ",
        "-----------------------------------------------------------------------------------------------------------",
        "Contoso Notes      ARP\\User\\X64\\fedcba9876543210fedcba9876543210            1.0                            ",
        "Git                Git.Git                                                  2.45.1       2.46.0     winget ",
        "Lecteur multimédia MSIX\\Microsoft.ZuneMusic_11.2406.12.0_x64__8wekyb3d8bbwe 11.2406.12.0                   ",
        "Paint Studio       9PCFS5B6T72H                                             5.0.13                  msstore",
    ];

    pub(crate) const VERSIONS_FR: &[&str] =
        &["Trouvé Contoso Editor [Contoso.Editor]", "Version", "-------", "2.10.0", "2.9.1", "2.9.0", "1.0.0-beta"];

    const CJK: &[&str] = &[
        "名前             ID                     バージョン 利用可能 ソース",
        "------------------------------------------------------------------",
        "日本語エディター Contoso.JapaneseEditor 1.0.0      1.1.0    winget",
        "Café · Ünïcödé…  Fabrikam.Unicode       2.0        2.1      winget",
        "한국어 프로그램  Contoso.Korean         3.0        3.5.1    winget",
        "Emoji 👍🏽 App     Contoso.Emoji          1.2        1.3      winget",
        "Flag 🇫🇷 App       Contoso.Flag           4.0        4.1      winget",
        "Family 👨\u{200d}👩\u{200d}👧 App    Contoso.Family         0.9        1.0      winget",
    ];

    fn rows(table: &Table) -> Vec<Vec<&str>> {
        table.rows.iter().map(|row| row.iter().map(String::as_str).collect()).collect()
    }

    #[test]
    fn french_upgrade_table() {
        let tables = parse_tables(&crlf(UPGRADE_FR));
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].headers, ["Nom", "ID", "Version", "Disponible", "Source"]);
        assert_eq!(
            rows(&tables[0]),
            [
                ["Git", "Git.Git", "2.45.1", "2.46.0", "winget"],
                ["Notepad++ (64-bit x64)", "Notepad++.Notepad++", "8.6.4", "8.6.9", "winget"],
                ["Contoso Sync (Utilisateur actuel)", "Contoso.Sync", "Unknown", "2.0.2", "winget"],
                ["Fabrikam Reader", "Fabrikam.Reader", "< 3.2.0", "3.4.1", "winget"],
                ["Terminal Windows", "9N0DX20HK701", "1.20.1", "1.21.2", "msstore"],
            ]
        );
    }

    #[test]
    fn lf_only_output() {
        let tables = parse_tables(&UPGRADE_FR.join("\n"));
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows.len(), 5);
    }

    #[test]
    fn spinner_garbage_and_explicit_targeting_table() {
        let tables = parse_tables(&crlf(UPGRADE_EN_EXPLICIT));
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].headers, ["Name", "Id", "Version", "Available", "Source"]);
        assert_eq!(
            rows(&tables[0]),
            [
                ["7-Zip 23.01 (x64)", "7zip.7zip", "23.01", "24.08", "winget"],
                ["Mozilla Firefox (x64 en-US)", "Mozilla.Firefox", "128.0", "129.0.1", "winget"],
                ["PowerToys (Preview)", "Microsoft.PowerToys", "0.82.1", "0.83.0", "winget"],
            ]
        );
        assert_eq!(
            rows(&tables[1]),
            [[
                "Microsoft Edge WebView2 Runtime",
                "Microsoft.EdgeWebView2Runtime",
                "127.0.2651.74",
                "127.0.2651.86",
                "winget"
            ]]
        );
    }

    #[test]
    fn list_with_pseudo_ids_and_empty_cells() {
        let tables = parse_tables(&crlf(LIST_FR_SOURCE));
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].headers, ["Nom", "ID", "Version", "Source"]);
        assert_eq!(
            rows(&tables[0]),
            [
                ["Accueil · Exemple", "ARP\\User\\X64\\0123456789abcdef0123456789abcdef", "1.0", ""],
                ["Café Crème Studio", "Contoso.CafeCreme", "3.1.4", "winget"],
                [
                    "Calculatrice Windows",
                    "MSIX\\Microsoft.WindowsCalculator_11.2405.2.0_x64__8wekyb3d8bbwe",
                    "11.2405.2.0",
                    ""
                ],
                ["Fabrikam Backup", "ARP\\Machine\\X64\\{01234567-89AB-CDEF-0123-456789ABCDEF}", "> 4.2", ""],
                ["Paint Studio", "9PCFS5B6T72H", "5.0.13", "msstore"],
                ["Unknown Tool", "Contoso.UnknownTool", "Unknown", "winget"],
            ]
        );
    }

    #[test]
    fn pseudo_ids_with_spaces() {
        let text = crlf(&[
            "Nom            ID                                                  Version Source",
            "---------------------------------------------------------------------------------",
            "Contoso Studio ARP\\Machine\\X64\\Contoso Studio                      2.1.0",
            "Fabrikam       Fabrikam.App                                        2.45.1  winget",
            "Outil réseau   ARP\\User\\X64\\{0A1B2C3D-0000-1111-2222-333344445555} 10.2",
        ]);
        let tables = parse_tables(&text);
        assert_eq!(
            rows(&tables[0]),
            [
                ["Contoso Studio", "ARP\\Machine\\X64\\Contoso Studio", "2.1.0", ""],
                ["Fabrikam", "Fabrikam.App", "2.45.1", "winget"],
                ["Outil réseau", "ARP\\User\\X64\\{0A1B2C3D-0000-1111-2222-333344445555}", "10.2", ""],
            ]
        );
    }

    #[test]
    fn five_columns_with_sparse_values() {
        let tables = parse_tables(&crlf(LIST_FR_5));
        assert_eq!(tables[0].headers.len(), 5);
        assert_eq!(
            rows(&tables[0]),
            [
                ["Contoso Notes", "ARP\\User\\X64\\fedcba9876543210fedcba9876543210", "1.0", "", ""],
                ["Git", "Git.Git", "2.45.1", "2.46.0", "winget"],
                [
                    "Lecteur multimédia",
                    "MSIX\\Microsoft.ZuneMusic_11.2406.12.0_x64__8wekyb3d8bbwe",
                    "11.2406.12.0",
                    "",
                    ""
                ],
                ["Paint Studio", "9PCFS5B6T72H", "5.0.13", "", "msstore"],
            ]
        );
    }

    #[test]
    fn missing_trailing_cells() {
        let tables = parse_tables(&crlf(LIST_EN_AVAILABLE));
        assert_eq!(rows(&tables[0])[1], ["Node.js LTS", "OpenJS.NodeJS.LTS", "20.15.0", ""]);
    }

    #[test]
    fn wide_characters_use_display_width() {
        let tables = parse_tables(&crlf(CJK));
        assert_eq!(tables[0].headers, ["名前", "ID", "バージョン", "利用可能", "ソース"]);
        assert_eq!(
            rows(&tables[0]),
            [
                ["日本語エディター", "Contoso.JapaneseEditor", "1.0.0", "1.1.0", "winget"],
                ["Café · Ünïcödé…", "Fabrikam.Unicode", "2.0", "2.1", "winget"],
                ["한국어 프로그램", "Contoso.Korean", "3.0", "3.5.1", "winget"],
                ["Emoji 👍🏽 App", "Contoso.Emoji", "1.2", "1.3", "winget"],
                ["Flag 🇫🇷 App", "Contoso.Flag", "4.0", "4.1", "winget"],
                ["Family 👨\u{200d}👩\u{200d}👧 App", "Contoso.Family", "0.9", "1.0", "winget"],
            ]
        );
    }

    #[test]
    fn combining_marks_stay_in_their_cell() {
        // "Cafe\u{301}" is 4 columns wide, like "Café".
        let text = crlf(&["Name  Id   Version", "------------------", "Cafe\u{301}  C.A  1.0", "Café  C.B  2.0"]);
        let tables = parse_tables(&text);
        assert_eq!(rows(&tables[0]), [["Cafe\u{301}", "C.A", "1.0"], ["Café", "C.B", "2.0"]]);
    }

    #[test]
    fn truncated_cells_are_kept() {
        let text = crlf(&[
            "Name                                     Id                                       Version       Available     Source",
            "--------------------------------------------------------------------------------------------------------------------",
            "Microsoft Visual C++ 2015-2022 Redistri… Microsoft.VCRedist.2015+.x64             14.38.33135.0 14.40.33810.0 winget",
            "Some Very Long Application Name That Is… Contoso.SomeVeryLongApplicationNameThat… 1.0           2.0           winget",
        ]);
        let tables = parse_tables(&text);
        assert_eq!(
            rows(&tables[0]),
            [
                [
                    "Microsoft Visual C++ 2015-2022 Redistri…",
                    "Microsoft.VCRedist.2015+.x64",
                    "14.38.33135.0",
                    "14.40.33810.0",
                    "winget"
                ],
                [
                    "Some Very Long Application Name That Is…",
                    "Contoso.SomeVeryLongApplicationNameThat…",
                    "1.0",
                    "2.0",
                    "winget"
                ],
            ]
        );
    }

    #[test]
    fn multi_word_header_crossed_by_values() {
        let text = crlf(&[
            "이름               ID                         버전    사용 가능 원본",
            "----------------------------------------------------------------------",
            "Visual Studio Code Microsoft.VisualStudioCode 1.141.2 1.142.0   winget",
            "메모장 플러스      Contoso.NotepadPlus        8.6.4   8.6.9     winget",
        ]);
        let tables = parse_tables(&text);
        assert_eq!(tables[0].headers, ["이름", "ID", "버전", "사용 가능", "원본"]);
        assert_eq!(
            rows(&tables[0]),
            [
                ["Visual Studio Code", "Microsoft.VisualStudioCode", "1.141.2", "1.142.0", "winget"],
                ["메모장 플러스", "Contoso.NotepadPlus", "8.6.4", "8.6.9", "winget"],
            ]
        );
    }

    #[test]
    fn multi_word_header_with_sparse_values() {
        let text = crlf(&[
            "이름    ID                                                              버전        사용 가능 원본  ",
            "----------------------------------------------------------------------------------------------------",
            "Git     Git.Git                                                         2.45.1      2.46.0.1  winget",
            "계산기  MSIX\\Microsoft.WindowsCalculator_11.2405.2.0_x64__8wekyb3d8bbwe 11.2405.2.0                 ",
            "Contoso Contoso.App                                                     1.0                   winget",
        ]);
        let tables = parse_tables(&text);
        assert_eq!(tables[0].headers.len(), 5);
        assert_eq!(rows(&tables[0])[0], ["Git", "Git.Git", "2.45.1", "2.46.0.1", "winget"]);
        assert_eq!(rows(&tables[0])[2], ["Contoso", "Contoso.App", "1.0", "", "winget"]);
    }

    #[test]
    fn multi_word_header_with_short_values() {
        let text = crlf(&[
            "Name    Id          Version Pin type",
            "------------------------------------",
            "Git     Git.Git     2.45.1  Pin",
            "Contoso Contoso.App 1.0     Pin",
        ]);
        let tables = parse_tables(&text);
        assert_eq!(tables[0].headers, ["Name", "Id", "Version", "Pin type"]);
        assert_eq!(rows(&tables[0])[0], ["Git", "Git.Git", "2.45.1", "Pin"]);
    }

    #[test]
    fn pin_table() {
        let text = crlf(&[
            "Name         Id           Version Source Pin type",
            "-------------------------------------------------",
            "Git          Git.Git      2.45.1  winget Pinning",
            "Contoso Tool Contoso.Tool 1.2.0   winget Blocking",
        ]);
        let tables = parse_tables(&text);
        assert_eq!(tables[0].headers, ["Name", "Id", "Version", "Source", "Pin type"]);
        assert_eq!(rows(&tables[0])[1], ["Contoso Tool", "Contoso.Tool", "1.2.0", "winget", "Blocking"]);
    }

    #[test]
    fn single_column_versions_table() {
        let tables = parse_tables(&crlf(VERSIONS_FR));
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].headers, ["Version"]);
        assert_eq!(rows(&tables[0]), [["2.10.0"], ["2.9.1"], ["2.9.0"], ["1.0.0-beta"]]);
    }

    #[test]
    fn match_column_with_spaces() {
        let text = crlf(&[
            "Name           Id             Version Match        Source",
            "---------------------------------------------------------",
            "Contoso Editor Contoso.Editor 2.10.0  Moniker: ced winget",
            "Fabrikam Code  Fabrikam.Code  1.4     Tag: ced     winget",
        ]);
        let tables = parse_tables(&text);
        assert_eq!(rows(&tables[0])[0], ["Contoso Editor", "Contoso.Editor", "2.10.0", "Moniker: ced", "winget"]);
        assert_eq!(rows(&tables[0])[1], ["Fabrikam Code", "Fabrikam.Code", "1.4", "Tag: ced", "winget"]);
    }

    #[test]
    fn powershell_dash_groups() {
        let text = crlf(&[
            "",
            "Name    Installed Version Latest Version   Missing Dependencies Info",
            "----    ----------------- --------------   -------------------- ----",
            "7zip    23.01             24.08",
            "python  3.12.4            3.12.5                                Held package",
            "",
        ]);
        let tables = parse_tables(&text);
        assert_eq!(tables[0].headers, ["Name", "Installed Version", "Latest Version", "Missing Dependencies", "Info"]);
        assert_eq!(
            rows(&tables[0]),
            [["7zip", "23.01", "24.08", "", ""], ["python", "3.12.4", "3.12.5", "", "Held package"]]
        );
    }

    #[test]
    fn no_table() {
        assert!(parse_tables("Aucune épingle n’est configurée.\r\n").is_empty());
        assert!(parse_tables("").is_empty());
        assert!(parse_tables("-------\r\n").is_empty());
        assert!(parse_tables("No installed package found matching input criteria.\r\n").is_empty());
    }

    #[test]
    fn footer_sentences_end_the_table() {
        let text = crlf(&[
            "Name   Id         Version Available Source",
            "------------------------------------------",
            "Git    Git.Git    2.45.1  2.46.0    winget",
            "1 package(s) have version numbers that cannot be determined. Use --include-unknown to see all results.",
            "Contoso Contoso.App 1.0 2.0 winget",
        ]);
        let tables = parse_tables(&text);
        assert_eq!(tables.len(), 1);
        assert_eq!(rows(&tables[0]), [["Git", "Git.Git", "2.45.1", "2.46.0", "winget"]]);
    }

    #[test]
    fn header_without_rows() {
        let tables = parse_tables(&crlf(&["Name Id Version", "---------------", "0 upgrades available."]));
        assert_eq!(tables.len(), 1);
        assert!(tables[0].rows.is_empty());
    }

    #[test]
    fn byte_order_mark_and_ansi_sequences() {
        let text = format!(
            "\u{feff}\u{1b}[0m{}",
            crlf(&["Name Id      Version", "--------------------", "Git  Git.Git 2.45.1"])
        );
        let tables = parse_tables(&text);
        assert_eq!(rows(&tables[0]), [["Git", "Git.Git", "2.45.1"]]);
    }
}
