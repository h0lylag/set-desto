use std::collections::HashSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FobScoutImport {
    pub rows: Vec<FobScoutRow>,
    pub messages: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FobScoutRow {
    pub import_index: usize,
    pub system: String,
    pub region: String,
    pub last_seen_utc: String,
    pub claimed_by: String,
}

pub fn parse_export(input: &str) -> FobScoutImport {
    let mut import = FobScoutImport::default();
    let lines: Vec<&str> = input.lines().collect();
    let Some(header_index) = lines.iter().position(|line| is_header_line(line)) else {
        if !input.trim().is_empty() {
            import
                .messages
                .push("No FOBScout system table was found".to_string());
        }
        return import;
    };

    let mut seen_systems = HashSet::new();
    let mut import_index = 0;

    for (line_index, line) in lines.iter().enumerate().skip(header_index + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if !trimmed.contains('|') {
            break;
        }
        if is_separator_line(trimmed) {
            continue;
        }

        let columns: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if columns.len() < 4 {
            import.messages.push(format!(
                "Line {} was skipped because it did not include all table columns",
                line_index + 1
            ));
            continue;
        }

        let system = columns[0].trim();
        if system.is_empty() {
            import.messages.push(format!(
                "Line {} was skipped because the system was empty",
                line_index + 1
            ));
            continue;
        }

        let normalized = system.to_ascii_lowercase();
        if !seen_systems.insert(normalized) {
            import.messages.push(format!(
                "{system} appeared more than once and was de-duplicated"
            ));
            continue;
        }

        import.rows.push(FobScoutRow {
            import_index,
            system: system.to_string(),
            region: columns[1].to_string(),
            last_seen_utc: columns[2].to_string(),
            claimed_by: columns[3].to_string(),
        });
        import_index += 1;
    }

    if import.rows.is_empty() && input.trim().is_empty() {
        import
            .messages
            .push("Paste a FOBScout export to preview systems".to_string());
    }

    import
}

fn is_header_line(line: &str) -> bool {
    let normalized = line
        .trim()
        .trim_matches('|')
        .split('|')
        .map(|column| column.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();

    normalized.len() >= 4
        && normalized[0] == "system"
        && normalized[1] == "region"
        && normalized[2] == "last seen utc"
        && normalized[3] == "claimed by"
}

fn is_separator_line(line: &str) -> bool {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .filter(|column| !column.is_empty())
        .all(|column| column.chars().all(|character| character == '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_EXPORT: &str = r#"FOBScout Export @ 2026-05-19 18:40:07 requested by Chudnor
Active FOBs: 2

System | Region | Last Seen UTC | Claimed By
------ | ------ | ------------- | ----------
Baratar | Khanid | 2026-05-19T18:38:54+00:00 | Unclaimed
Daran | Kor-Azor | 2026-05-19T18:22:30+00:00 | Chudnor
"#;

    #[test]
    fn parses_fobscout_export_rows() {
        let import = parse_export(SAMPLE_EXPORT);

        assert_eq!(import.rows.len(), 2);
        assert_eq!(import.rows[0].system, "Baratar");
        assert_eq!(import.rows[0].region, "Khanid");
        assert_eq!(import.rows[0].claimed_by, "Unclaimed");
        assert_eq!(import.rows[1].system, "Daran");
        assert!(import.messages.is_empty());
    }

    #[test]
    fn tolerates_wrapping_pipes_and_spacing() {
        let import = parse_export(
            r#"| System | Region | Last Seen UTC | Claimed By |
| ------ | ------ | ------------- | ---------- |
| Jita | The Forge | 2026-05-19T18:38:54+00:00 | Unclaimed |
"#,
        );

        assert_eq!(import.rows.len(), 1);
        assert_eq!(import.rows[0].system, "Jita");
    }

    #[test]
    fn de_duplicates_systems_case_insensitively() {
        let import = parse_export(
            r#"System | Region | Last Seen UTC | Claimed By
------ | ------ | ------------- | ----------
Jita | The Forge | 2026-05-19T18:38:54+00:00 | Unclaimed
jita | The Forge | 2026-05-19T18:39:54+00:00 | Unclaimed
"#,
        );

        assert_eq!(import.rows.len(), 1);
        assert!(import.messages[0].contains("de-duplicated"));
    }

    #[test]
    fn reports_missing_table() {
        let import = parse_export("hello");

        assert!(import.rows.is_empty());
        assert!(import.messages[0].contains("No FOBScout system table"));
    }

    #[test]
    fn reports_malformed_rows() {
        let import = parse_export(
            r#"System | Region | Last Seen UTC | Claimed By
------ | ------ | ------------- | ----------
Jita | The Forge
"#,
        );

        assert!(import.rows.is_empty());
        assert!(import.messages[0].contains("did not include all table columns"));
    }

    #[test]
    fn empty_input_prompts_for_paste() {
        let import = parse_export("");

        assert!(import.rows.is_empty());
        assert!(import.messages.is_empty());
    }
}
