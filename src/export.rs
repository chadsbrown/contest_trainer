use crate::config::AppSettings;
use crate::stats::SessionStats;
use chrono::Local;
use std::fs::File;
use std::io::Write;

/// Export session statistics to a markdown file in the current directory.
/// Returns Ok(filename) on success, Err(error_message) on failure.
pub fn export_session_stats(
    settings: &AppSettings,
    stats: &SessionStats,
) -> Result<String, String> {
    let now = Local::now();
    let callsign = settings.user.callsign.trim();
    let callsign_safe = if callsign.is_empty() {
        "NOCALL".to_string()
    } else {
        callsign.to_uppercase()
    };

    let filename = format!("CWCT-{}-{}.md", callsign_safe, now.format("%Y%m%d-%H%M"));
    let content = build_markdown_content(settings, stats);

    let mut file = File::create(&filename).map_err(|e| format!("Failed to create file: {}", e))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(filename)
}

fn build_markdown_content(settings: &AppSettings, stats: &SessionStats) -> String {
    let now = Local::now();
    let analysis = stats.analyze();
    let mut md = String::new();

    // Header
    md.push_str("# CWCT Session Export\n\n");
    md.push_str(&format!("**Callsign:** {}  \n", settings.user.callsign));
    md.push_str(&format!(
        "**Exported:** {}\n\n",
        now.format("%Y-%m-%d %H:%M")
    ));

    // Session Summary
    md.push_str("## Session Summary\n\n");
    md.push_str(&format!("- Total QSOs: {}\n", analysis.total_qsos));
    md.push_str(&format!(
        "- Correct QSOs: {} ({:.1}%)\n",
        analysis.correct_qsos, analysis.correct_rate
    ));
    md.push_str(&format!("- Total Points: {}\n\n", analysis.total_points));

    // Accuracy
    md.push_str("## Accuracy\n\n");
    md.push_str(&format!(
        "- Callsign Accuracy: {}/{} ({:.1}%)\n",
        analysis.correct_callsigns, analysis.total_qsos, analysis.callsign_accuracy
    ));
    md.push_str(&format!(
        "- Exchange Accuracy: {}/{} ({:.1}%)\n\n",
        analysis.correct_exchanges, analysis.total_qsos, analysis.exchange_accuracy
    ));

    // Streaks
    md.push_str("## Streaks\n\n");
    md.push_str(&format!(
        "- Current Clean: {}\n",
        analysis.streaks.current_clean
    ));
    md.push_str(&format!("- Max Clean: {}\n", analysis.streaks.max_clean));
    md.push_str(&format!(
        "- Current Error: {}\n",
        analysis.streaks.current_error
    ));
    md.push_str(&format!("- Max Error: {}\n\n", analysis.streaks.max_error));

    // F5/F8 Usage
    md.push_str("## F5/F8 Usage\n\n");
    md.push_str(&format!(
        "- F5 (His Call): {}\n",
        analysis.f5_callsign_count
    ));
    md.push_str(&format!("- F8 Callsign: {}\n", analysis.agn_callsign_count));
    md.push_str(&format!("- F8 Exchange: {}\n", analysis.agn_exchange_count));
    if analysis.total_qsos > 0 {
        let agn_pct = (analysis.agn_any_count as f32 / analysis.total_qsos as f32) * 100.0;
        md.push_str(&format!(
            "- Total with F8: {} ({:.1}%)\n\n",
            analysis.agn_any_count, agn_pct
        ));
    } else {
        md.push_str(&format!("- Total with F8: {}\n\n", analysis.agn_any_count));
    }

    // Calling Station Speed
    md.push_str("## Calling Station Speed\n\n");
    if analysis.total_qsos > 0 {
        md.push_str(&format!("- Average WPM: {:.1}\n", analysis.avg_station_wpm));
        md.push_str(&format!(
            "- WPM Range: {} - {}\n\n",
            analysis.min_station_wpm, analysis.max_station_wpm
        ));
    } else {
        md.push_str("No QSOs logged yet.\n\n");
    }

    // WPM Accuracy buckets
    md.push_str("## WPM Accuracy (2-WPM buckets)\n\n");
    if analysis.wpm_buckets.is_empty() {
        md.push_str("No QSOs logged yet.\n\n");
    } else {
        md.push_str("| Bucket | Total | Correct | Accuracy |\n");
        md.push_str("|--------|-------|---------|----------|\n");
        for bucket in &analysis.wpm_buckets {
            md.push_str(&format!(
                "| {} | {} | {} | {:.1}% |\n",
                bucket.label, bucket.total, bucket.correct, bucket.accuracy_pct
            ));
        }
        md.push('\n');
    }

    // Character Error Analysis
    md.push_str("## Character Error Analysis\n\n");
    let errors_with_rate: Vec<_> = analysis
        .char_error_rates
        .iter()
        .filter(|(_, rate, _)| *rate > 0.0)
        .take(10)
        .collect();
    if errors_with_rate.is_empty() {
        md.push_str("No character errors recorded.\n\n");
    } else {
        md.push_str("| Char | Error Rate | Samples |\n");
        md.push_str("|------|------------|--------|\n");
        for (ch, error_rate, count) in errors_with_rate {
            let char_display = if *ch == ' ' {
                "[space]".to_string()
            } else {
                ch.to_string()
            };
            md.push_str(&format!(
                "| {} | {:.1}% | {} |\n",
                char_display, error_rate, count
            ));
        }
        md.push('\n');
    }

    // QSO Log table with all QsoRecord fields
    md.push_str("## QSO Log\n\n");
    if stats.qsos.is_empty() {
        md.push_str("No QSOs logged yet.\n");
    } else {
        md.push_str("| # | Expected Call | Entered Call | Call OK | Expected Exch | Entered Exch | Exch OK | WPM | Points | AGN Call | AGN Exch | F5 Used |\n");
        md.push_str("|---|---------------|--------------|---------|---------------|--------------|---------|-----|--------|----------|----------|--------|\n");
        for (i, qso) in stats.qsos.iter().enumerate() {
            let call_ok = if qso.callsign_correct { "Yes" } else { "No" };
            let exch_ok = if qso.exchange_correct { "Yes" } else { "No" };
            let agn_call = if qso.used_agn_callsign { "Yes" } else { "No" };
            let agn_exch = if qso.used_agn_exchange { "Yes" } else { "No" };
            let f5_used = if qso.used_f5_callsign { "Yes" } else { "No" };

            md.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                i + 1,
                qso.expected_callsign,
                qso.entered_callsign,
                call_ok,
                qso.expected_exchange,
                qso.entered_exchange,
                exch_ok,
                qso.station_wpm,
                qso.points,
                agn_call,
                agn_exch,
                f5_used
            ));
        }
    }

    md
}
