//! ExecuteToolCallBlock - runs shell commands with streaming output.
use ratatui::style::Modifier;
use ratatui::text::{Line, Span, Text};

use super::TOOL_HEADER_RANGE;
use crate::appearance::ExecuteHeaderStyle;
use crate::i18n;
use crate::render::wrapping::word_wrap_lines_with_joiners;
use crate::scrollback::block::BlockContent;
use crate::scrollback::types::{
    AccentStyle, BlockBackground, BlockContext, BlockLine, BlockOutput, DisplayMode, Selectable,
};
use crate::theme::Theme;

/// Memory guardrail for a single execute block's raw output. Streaming a
/// multi-hundred-MB log into scrollback would pin that memory for the whole
/// session (scrollback keeps every entry until evicted by the 8192-entry cap).
/// Once the guardrail trips, only a bounded head plus a rolling trailing
/// window are retained in memory, with an explicit omission marker; the full
/// output remains available on disk via the tool's `output_file`. Selection,
/// search and expand all operate on the retained portion and show the marker.
pub(crate) const EXECUTE_OUTPUT_MAX_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const EXECUTE_OUTPUT_KEEP_HEAD_BYTES: usize = 256 * 1024;
pub(crate) const EXECUTE_OUTPUT_KEEP_TAIL_BYTES: usize = 64 * 1024;

fn utf8_head(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn utf8_tail(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut start = s.len() - max;
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    &s[start..]
}

fn guardrail_omission_marker(omitted: usize) -> String {
    format!(
        "\n… [{omitted} bytes omitted in memory; full output on disk via the tool output file]\n"
    )
}

const EXECUTE_STDOUT_RANGE_BASE: u16 = 1;

/// Execute tool call - runs a shell command.
#[derive(Debug, Clone)]
pub struct ExecuteToolCallBlock {
    /// Full command that was run (search / copy_meta / export source of truth).
    pub command: String,
    /// Error message if the command failed (None = success).
    pub error: Option<String>,
    /// Optional description of what the command does.
    pub description: Option<String>,
    /// The terminal output. Streamed incrementally.
    pub output: Option<String>,
    /// When the tool started running (Phase 2: time tracking).
    pub started_at: Option<std::time::Instant>,
    /// Elapsed time in ms after completion (Phase 2: time tracking).
    pub elapsed_ms: Option<i64>,
    /// Whether this is a user-initiated bash-mode (`!`) command.
    /// Streams as a truncated live tail, expands to full output on finish.
    pub bash_mode: bool,
    /// Peeled display form for the header; `command` stays the full source of truth.
    pub header_display: Option<String>,
    /// Total raw bytes appended so far. Drives the memory guardrail; when it
    /// trips, `output` stops being the full stream (see module docs).
    output_total_bytes: usize,
    /// True once the guardrail tripped: `output` holds head + marker + rolling
    /// tail window, and `output_tail_start` is the tail window's offset.
    output_guarded: bool,
    /// Byte offset of the rolling tail window inside `output` when guarded.
    output_tail_start: usize,
}
impl ExecuteToolCallBlock {
    /// Create a new execute block.
    ///
    /// `started_at` defaults to `None`. For streaming blocks, timing begins
    /// when the block enters running UI state (via `start_timing()`).
    /// Pre-completed blocks never get timing — they show `"—"`.
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            error: None,
            description: None,
            output: None,
            started_at: None,
            elapsed_ms: None,
            bash_mode: false,
            header_display: None,
            output_total_bytes: 0,
            output_guarded: false,
            output_tail_start: 0,
        }
    }

    /// Set error (marks as failed).
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Set description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set output.
    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        let output = output.into();
        self.replace_output(&output);
        self
    }

    /// Push streaming output chunk.
    pub fn push_output(&mut self, chunk: &str) {
        self.output_total_bytes = self.output_total_bytes.saturating_add(chunk.len());
        if self.output_guarded {
            // Keep the rolling tail window bounded; the head + marker prefix
            // is already fixed in place before `output_tail_start`.
            let mut out = self.output.take().unwrap_or_default();
            out.push_str(chunk);
            let tail_len = out.len() - self.output_tail_start;
            if tail_len > EXECUTE_OUTPUT_KEEP_TAIL_BYTES {
                let drop = tail_len - EXECUTE_OUTPUT_KEEP_TAIL_BYTES;
                let mut drop_end = self.output_tail_start + drop;
                while drop_end < out.len() && !out.is_char_boundary(drop_end) {
                    drop_end += 1;
                }
                out.drain(self.output_tail_start..drop_end);
            }
            self.output = Some(out);
            return;
        }
        if self.output_total_bytes <= EXECUTE_OUTPUT_MAX_BYTES {
            match &mut self.output {
                Some(o) => o.push_str(chunk),
                None => self.output = Some(chunk.to_string()),
            }
            return;
        }
        // Trip the guardrail: collapse the accumulated buffer to head +
        // marker + tail window, then keep the tail rolling from here on.
        self.output_guarded = true;
        let full = self.output.take().unwrap_or_default();
        let head = utf8_head(&full, EXECUTE_OUTPUT_KEEP_HEAD_BYTES);
        let omitted = self
            .output_total_bytes
            .saturating_sub(head.len() + EXECUTE_OUTPUT_KEEP_TAIL_BYTES);
        let mut tail = utf8_tail(&full, EXECUTE_OUTPUT_KEEP_TAIL_BYTES).to_owned();
        tail.push_str(chunk);
        if tail.len() > EXECUTE_OUTPUT_KEEP_TAIL_BYTES {
            tail = utf8_tail(&tail, EXECUTE_OUTPUT_KEEP_TAIL_BYTES).to_owned();
        }
        let mut out = String::with_capacity(head.len() + tail.len() + 96);
        out.push_str(head);
        out.push_str(&guardrail_omission_marker(omitted));
        self.output_tail_start = out.len();
        out.push_str(&tail);
        self.output = Some(out);
    }

    /// Replace the output wholesale (non-incremental streaming path: the shell
    /// sends the full accumulated buffer each tick). Applies the same memory
    /// guardrail as [`Self::push_output`].
    pub fn replace_output(&mut self, output: &str) {
        self.output_total_bytes = output.len();
        if output.len() <= EXECUTE_OUTPUT_MAX_BYTES {
            self.output_guarded = false;
            self.output_tail_start = 0;
            self.output = Some(output.to_owned());
            return;
        }
        self.output_guarded = true;
        let head = utf8_head(output, EXECUTE_OUTPUT_KEEP_HEAD_BYTES);
        let omitted = output
            .len()
            .saturating_sub(head.len() + EXECUTE_OUTPUT_KEEP_TAIL_BYTES);
        let tail = utf8_tail(output, EXECUTE_OUTPUT_KEEP_TAIL_BYTES);
        let mut out = String::with_capacity(head.len() + tail.len() + 96);
        out.push_str(head);
        out.push_str(&guardrail_omission_marker(omitted));
        self.output_tail_start = out.len();
        out.push_str(tail);
        self.output = Some(out);
    }

    /// Finalize elapsed time from `started_at`.
    ///
    /// Idempotent: no-op if `started_at` is `None` (pre-completed block)
    /// or if `elapsed_ms` is already set (already finalized).
    pub fn finish(&mut self) {
        if self.elapsed_ms.is_some() {
            return;
        }
        if let Some(start) = self.started_at {
            self.elapsed_ms = Some(start.elapsed().as_millis() as i64);
        }
    }

    /// Set error (mutable) — compute elapsed time if not already set (Phase 2).
    pub fn set_error(&mut self, error: Option<String>) {
        if self.elapsed_ms.is_none()
            && let Some(start) = self.started_at
        {
            self.elapsed_ms = Some(start.elapsed().as_millis() as i64);
        }
        self.error = error;
    }

    /// Check if successful (no error).
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    /// Get elapsed time in ms. Returns current elapsed if still running, or stored value if finished (Phase 2).
    pub fn elapsed_ms(&self) -> Option<i64> {
        match self.elapsed_ms {
            Some(ms) => Some(ms),
            None => self
                .started_at
                .map(|start| start.elapsed().as_millis() as i64),
        }
    }

    /// Get copyable text for this block (stdout output).
    ///
    /// Returns the output string if available, or empty string if no output.
    pub fn copy_text(&self) -> String {
        self.output
            .as_deref()
            .map(crate::render::terminal_output::render_terminal_plain)
            .unwrap_or_default()
    }

    /// Display form of the command for the header (may peel `cd &&` prefix).
    ///
    /// Preserves physical newlines so soft-wrap / copy can keep line structure.
    /// Callers that need a single ratatui line must flatten themselves.
    fn command_display(&self) -> &str {
        self.header_display.as_deref().unwrap_or(&self.command)
    }

    /// Non-empty description for the header title, if the model supplied one.
    ///
    /// When `strip_run_prefix` is true (Label style already has a bold `Run `
    /// prefix), a leading `Run` / `Running` on the description is dropped so
    /// we never render `Run Run the tests`.
    fn description_display(&self, strip_run_prefix: bool) -> Option<String> {
        self.description.as_ref().and_then(|d| {
            let trimmed = d.trim();
            if trimmed.is_empty() {
                return None;
            }
            // Collapse newlines so the title stays one logical line
            // (`wrap_header_hanging` may still soft-wrap for width).
            let mut text = trimmed.replace('\n', " ");
            if strip_run_prefix {
                text = strip_leading_run_word(&text);
                if text.is_empty() {
                    return None;
                }
            }
            Some(text)
        })
    }

    /// `$ command` line (shell prompt style). Sole header when there is no
    /// description; secondary line when there is one.
    ///
    /// Flattens newlines for single-line layouts (collapsed truncate). Expanded
    /// rendering uses soft-wrap via [`Self::push_shell_command_soft_wrap`].
    fn shell_command_line(&self, theme: &Theme, muted_command: bool) -> Line<'static> {
        let command = self.command_display().replace('\n', " ");
        let command = if command.trim().is_empty() {
            "\u{2026}".to_string()
        } else {
            command
        };
        let command_spans: Vec<Span<'static>> = if muted_command || command == "\u{2026}" {
            vec![Span::styled(command, theme.muted())]
        } else {
            crate::views::tasks_pane::highlight_bash_command(&command)
        };
        // `$` uses gray_dim — dimmer than muted, no bold
        let mut spans = vec![Span::styled("$ ", theme.dim())];
        spans.extend(command_spans);
        Line::from(spans)
    }

    /// Prefix spans for a soft-wrapped command header (`$ ` or `Run [(user) ]`).
    ///
    /// Returns `(prefix_spans, hang_width)` — hang is the display width of the
    /// first-row prefix so continuations indent under the command body.
    fn command_header_prefix(
        &self,
        theme: &Theme,
        header_style: ExecuteHeaderStyle,
        muted_command: bool,
    ) -> (Vec<Span<'static>>, usize) {
        use unicode_width::UnicodeWidthStr;
        match header_style {
            ExecuteHeaderStyle::Shell => {
                let prefix = "$ ";
                (
                    vec![Span::styled(prefix.to_string(), theme.dim())],
                    UnicodeWidthStr::width(prefix),
                )
            }
            ExecuteHeaderStyle::Label => {
                let label_style = if muted_command {
                    theme.muted().add_modifier(Modifier::BOLD)
                } else {
                    theme.primary().add_modifier(Modifier::BOLD)
                };
                let mut spans = vec![Span::styled(i18n::tr("Run ").to_string(), label_style)];
                let mut hang = UnicodeWidthStr::width("Run ");
                if self.bash_mode {
                    spans.push(Span::styled("(user) ".to_string(), theme.muted()));
                    hang += UnicodeWidthStr::width("(user) ");
                }
                (spans, hang)
            }
        }
    }

    /// Multi-line command header using permission-panel soft-wrap (operators + quotes).
    ///
    /// Used for both Shell (`$ command`) and Label (`Run [(user) ]command`) so
    /// physical newlines / `\` continuations match the permission overlay.
    fn push_command_soft_wrap(
        &self,
        lines: &mut Vec<BlockLine>,
        theme: &Theme,
        header_style: ExecuteHeaderStyle,
        muted_command: bool,
        width: usize,
        extra_indent: usize,
    ) {
        let command = self.command_display();
        let command = if command.trim().is_empty() {
            "\u{2026}"
        } else {
            command
        };
        let (prefix_spans, prefix_w) =
            self.command_header_prefix(theme, header_style, muted_command);
        let prefix_span_count = prefix_spans.len();
        let hang = extra_indent.saturating_add(prefix_w);
        let cmd_width = width.saturating_sub(hang).max(1);

        let cmd_rows: Vec<Line<'static>> = if muted_command || command == "\u{2026}" {
            let style = theme.muted();
            let flat = command.replace('\n', " ");
            let line = Line::from(Span::styled(flat, style));
            crate::render::wrapping::word_wrap_lines(std::iter::once(line), cmd_width)
        } else {
            crate::views::permission_view::render_bash_command_display_lines(command, cmd_width)
        };

        let hang_indent: String = " ".repeat(hang);
        if cmd_rows.is_empty() {
            lines.push(BlockLine {
                selectable: Selectable::None,
                selection_range: Some(TOOL_HEADER_RANGE),
                content: Line::from(prefix_spans),
                ..Default::default()
            });
            return;
        }
        for (i, row) in cmd_rows.into_iter().enumerate() {
            let mut spans = if i == 0 {
                prefix_spans.clone()
            } else {
                vec![Span::raw(hang_indent.clone())]
            };
            spans.extend(row.spans);
            let total = spans.len();
            let select_start = if i == 0 {
                prefix_span_count.min(total)
            } else {
                0
            };
            lines.push(BlockLine {
                selectable: Selectable::Spans(select_start..total),
                selection_range: Some(TOOL_HEADER_RANGE),
                // Preserve physical/soft-wrap structure as newlines when copying.
                joiner: if i == 0 { None } else { Some("\n".to_string()) },
                content: Line::from(spans),
                ..Default::default()
            });
        }
    }

    /// Primary title line for Label style: `Run [(user) ]<description|command>`.
    ///
    /// When `title` is empty (eager placeholder before `raw_input.command`
    /// arrives), renders `Run …` so we never flash an internal tool id.
    fn label_title_line(
        &self,
        theme: &Theme,
        muted_command: bool,
        title: &str,
        highlight_as_command: bool,
    ) -> Line<'static> {
        let label_style = if muted_command {
            theme.muted().add_modifier(Modifier::BOLD)
        } else {
            theme.primary().add_modifier(Modifier::BOLD)
        };
        let mut spans = vec![Span::styled(i18n::tr("Run "), label_style)];
        if self.bash_mode {
            // Same style as session event messages (e.g. "Worked for 2.3s.")
            spans.push(Span::styled("(user) ", theme.muted()));
        }
        // Single ratatui Line — never pass raw newlines (callers that need
        // multi-line command display use `push_command_soft_wrap`).
        let title_owned;
        let title = if title.trim().is_empty() {
            "\u{2026}" // …
        } else if title.contains('\n') {
            title_owned = title.replace('\n', " ");
            title_owned.as_str()
        } else {
            title
        };
        if highlight_as_command && !muted_command && title != "\u{2026}" {
            spans.extend(crate::views::tasks_pane::highlight_bash_command(title));
        } else if muted_command {
            spans.push(Span::styled(title.to_string(), theme.muted()));
        } else {
            // Description (or loading ellipsis): plain primary text.
            spans.push(Span::styled(title.to_string(), theme.primary()));
        }
        Line::from(spans)
    }

    /// Header lines for the execute block (description-first when a description exists).
    ///
    /// Without description (unchanged):
    /// - Shell: `$ command`
    /// - Label: `Run [(user) ]command`
    ///
    /// With description:
    /// - Shell: description title; optionally `$ command` on the next line
    /// - Label: `Run [(user) ]description`; optionally `$ command` on the next line
    ///
    /// Collapsed mode passes `include_command = false` so only the description
    /// title is shown (density). Expanded/truncated include the command line.
    ///
    /// When `muted` is true, command/description body uses muted gray (collapsed).
    /// Uses precomputed `header_display` when set; `self.command` stays full.
    ///
    /// Returns `(line, prefix_span_count)` — prefix spans are not selectable
    /// (`"Run "` / `"(user) "` / `"$ "`).
    fn header_lines(
        &self,
        theme: &Theme,
        header_style: ExecuteHeaderStyle,
        muted_command: bool,
        include_command: bool,
    ) -> Vec<(Line<'static>, usize)> {
        let strip_run = matches!(header_style, ExecuteHeaderStyle::Label);
        match self.description_display(strip_run) {
            Some(desc) => {
                let title = match header_style {
                    ExecuteHeaderStyle::Label => {
                        let prefix_spans = if self.bash_mode { 2 } else { 1 };
                        (
                            self.label_title_line(theme, muted_command, &desc, false),
                            prefix_spans,
                        )
                    }
                    ExecuteHeaderStyle::Shell => {
                        // Description as plain title (no `$`); command follows when included.
                        let style = if muted_command {
                            theme.muted()
                        } else {
                            theme.primary()
                        };
                        (Line::from(Span::styled(desc, style)), 0)
                    }
                };
                if include_command {
                    let cmd = (self.shell_command_line(theme, muted_command), 1);
                    vec![title, cmd]
                } else {
                    vec![title]
                }
            }
            None => {
                // No description: single-line header (callers that soft-wrap the
                // command as title use `push_command_soft_wrap` instead).
                // Flatten newlines — raw `\n` is invalid inside one ratatui Line.
                let line = match header_style {
                    ExecuteHeaderStyle::Shell => self.shell_command_line(theme, muted_command),
                    ExecuteHeaderStyle::Label => {
                        let flat = self.command_display().replace('\n', " ");
                        self.label_title_line(theme, muted_command, &flat, true)
                    }
                };
                let prefix_spans = match header_style {
                    ExecuteHeaderStyle::Shell => 1,
                    ExecuteHeaderStyle::Label => {
                        if self.bash_mode {
                            2
                        } else {
                            1
                        }
                    }
                };
                vec![(line, prefix_spans)]
            }
        }
    }

    /// Push header `BlockLine`s, selecting non-prefix spans.
    ///
    /// When `truncate_to_width` is true (collapsed one-line budget), each
    /// logical header line is hard-truncated. Otherwise soft-wraps: **command**
    /// lines (Shell `$ …` and Label `Run …` when the command is the title, or
    /// the secondary `$ command` under a description) use permission-panel bash
    /// soft-wrap; description titles use hanging word-wrap.
    /// `include_command` is false in collapsed mode so a description title
    /// alone is shown without the command line.
    ///
    /// `width` must already be the block content width (bullet accounted for).
    /// Do not also pass bullet width as `extra_indent` — hang is only for the
    /// `$` / `Run` prefix.
    #[allow(clippy::too_many_arguments)]
    fn push_header_lines(
        &self,
        lines: &mut Vec<BlockLine>,
        theme: &Theme,
        header_style: ExecuteHeaderStyle,
        muted_command: bool,
        width: usize,
        extra_indent: usize,
        truncate_to_width: bool,
        include_command: bool,
    ) {
        let strip_run = matches!(header_style, ExecuteHeaderStyle::Label);
        // Expanded/truncated: command-as-title soft-wrap (Shell + Label).
        if !truncate_to_width && include_command && self.description_display(strip_run).is_none() {
            self.push_command_soft_wrap(
                lines,
                theme,
                header_style,
                muted_command,
                width,
                extra_indent,
            );
            return;
        }
        // Description title (word-wrap) then soft-wrapped `$ command` (both styles).
        if !truncate_to_width && include_command && self.description_display(strip_run).is_some() {
            let headers = self.header_lines(theme, header_style, muted_command, false);
            for (line, prefix_spans) in headers {
                let wrapped =
                    crate::render::wrapping::wrap_header_hanging(line, width, extra_indent);
                for (i, wrapped_line) in wrapped.into_iter().enumerate() {
                    let total = wrapped_line.spans.len();
                    let start = if i == 0 { prefix_spans.min(total) } else { 0 };
                    lines.push(BlockLine {
                        selectable: Selectable::Spans(start..total),
                        selection_range: Some(TOOL_HEADER_RANGE),
                        joiner: if i == 0 { None } else { Some(" ".to_string()) },
                        content: wrapped_line,
                        ..Default::default()
                    });
                }
            }
            // Secondary command line is always shell-style (`$ …`).
            self.push_command_soft_wrap(
                lines,
                theme,
                ExecuteHeaderStyle::Shell,
                muted_command,
                width,
                extra_indent,
            );
            return;
        }

        let headers = self.header_lines(theme, header_style, muted_command, include_command);
        for (line, prefix_spans) in headers {
            if truncate_to_width {
                let line = crate::render::line_utils::truncate_line(line, width);
                let total = line.spans.len();
                let start = prefix_spans.min(total);
                lines.push(BlockLine {
                    selectable: Selectable::Spans(start..total),
                    selection_range: Some(TOOL_HEADER_RANGE),
                    content: line,
                    ..Default::default()
                });
            } else {
                let wrapped =
                    crate::render::wrapping::wrap_header_hanging(line, width, extra_indent);
                for (i, wrapped_line) in wrapped.into_iter().enumerate() {
                    let total = wrapped_line.spans.len();
                    // Only the first wrapped segment of each logical header line
                    // has the prefix (`Run ` / `$ `); continuations are fully selectable.
                    let start = if i == 0 { prefix_spans.min(total) } else { 0 };
                    lines.push(BlockLine {
                        selectable: Selectable::Spans(start..total),
                        selection_range: Some(TOOL_HEADER_RANGE),
                        joiner: if i == 0 { None } else { Some(" ".to_string()) },
                        content: wrapped_line,
                        ..Default::default()
                    });
                }
            }
        }
    }

    /// Render with optional truncation.
    ///
    /// If `truncate` is Some((first, last)), shows first N lines, ellipsis, last M lines.
    /// If `truncate` is None, shows all output lines.
    fn render_with_truncation(
        &self,
        theme: &Theme,
        width: usize,
        truncate: Option<(usize, usize)>,
        header_style: ExecuteHeaderStyle,
        extra_indent: usize,
    ) -> BlockOutput {
        let mut lines: Vec<BlockLine> = Vec::new();
        self.push_header_lines(
            &mut lines,
            theme,
            header_style,
            false,
            width,
            extra_indent,
            false,
            true, // include $ command when expanded/truncated
        );

        if self.output.is_none()
            && let Some(error) = &self.error
            && !error.is_empty()
        {
            lines.push(BlockLine::separator(Line::from("")));
            let error_style = ratatui::style::Style::default().fg(theme.accent_error);
            for line in error.lines() {
                lines.push(BlockLine::separator(Line::from(Span::styled(
                    line.to_string(),
                    error_style,
                ))));
            }
        }

        if let Some(output) = &self.output
            && !output.is_empty()
        {
            lines.push(BlockLine::separator(Line::from("")));

            // Default foreground for the output body. Under the terminal-native
            // (minimal) palette, muted used to collapse to ANSI bright black and
            // washed out on many dark profiles; content must track the terminal's
            // default fg (`primary` / Reset). Labels / chrome stay on `muted`/`dim`.
            let styled_lines: Vec<Line<'static>> =
                crate::render::terminal_output::render_terminal_lines(output, theme.primary())
                    .into_iter()
                    .map(|rl| rl.line)
                    .collect();

            // Source-line pre-truncation: the truncated display only ever shows
            // `first` leading and `last` trailing WRAPPED lines, and the first
            // `first` wrapped lines can never need more than the first `first`
            // source lines (each source line produces >= 1 wrapped line; same
            // for the tail). When the source has far more lines than the
            // display budget, word-wrapping every line is pure waste: a
            // 100k-line build log would otherwise be soft-wrapped in full on
            // every streaming tick just to show 5 lines. We wrap only the
            // head/tail source lines and let the wrapped-level truncation
            // below keep the exact same visual semantics (a still-oversized
            // single source line is handled by the wrapped budget).
            let mut middle_skipped = false;
            let lines_for_wrap = match truncate {
                Some((first, last)) if styled_lines.len() > first + last => {
                    middle_skipped = true;
                    let head_end = first.min(styled_lines.len());
                    let tail_start = styled_lines.len().saturating_sub(last);
                    // `len > first + last` guarantees tail_start > head_end,
                    // so head/tail never overlap and ordering is preserved.
                    let mut kept = Vec::with_capacity(head_end + last);
                    kept.extend(styled_lines[..head_end].iter().cloned());
                    kept.extend(styled_lines[tail_start..].iter().cloned());
                    kept
                }
                _ => styled_lines,
            };

            let (wrapped, joiners) =
                word_wrap_lines_with_joiners(lines_for_wrap, width.saturating_sub(2).max(20));
            let total = wrapped.len();

            // Apply truncation if specified and content exceeds limits
            if let Some((first, last)) = truncate {
                let threshold = first + last;
                // `middle_skipped` forces the ellipsis path: the source was
                // pre-trimmed, so an ellipsis must tell the user lines were
                // omitted even when the wrapped total fits the budget.
                if total > threshold || middle_skipped {
                    // First N lines: stdout range base
                    for (wrapped_line, joiner) in wrapped.iter().zip(joiners.iter()).take(first) {
                        lines.push(
                            BlockLine::styled(wrapped_line.clone())
                                .with_panel_background(theme.bg_dark)
                                .with_selection_range(Some(EXECUTE_STDOUT_RANGE_BASE))
                                .with_joiner(joiner.clone()),
                        );
                    }
                    // Ellipsis (non-selectable, breaks range continuity)
                    lines.push(
                        BlockLine::separator(Line::from(Span::styled("\u{2026}", theme.muted())))
                            .with_panel_background(theme.bg_dark),
                    );
                    // Last M lines: range base + 1 (distinct from first chunk)
                    for (wrapped_line, joiner) in wrapped
                        .iter()
                        .zip(joiners.iter())
                        .skip(total.saturating_sub(last))
                    {
                        lines.push(
                            BlockLine::styled(wrapped_line.clone())
                                .with_panel_background(theme.bg_dark)
                                .with_selection_range(Some(EXECUTE_STDOUT_RANGE_BASE + 1))
                                .with_joiner(joiner.clone()),
                        );
                    }
                } else {
                    // Content fits, show all with same range
                    for (wrapped_line, joiner) in wrapped.into_iter().zip(joiners) {
                        lines.push(
                            BlockLine::styled(wrapped_line)
                                .with_panel_background(theme.bg_dark)
                                .with_selection_range(Some(EXECUTE_STDOUT_RANGE_BASE))
                                .with_joiner(joiner),
                        );
                    }
                }
            } else {
                // No truncation, show all with same range
                for (wrapped_line, joiner) in wrapped.into_iter().zip(joiners) {
                    lines.push(
                        BlockLine::styled(wrapped_line)
                            .with_panel_background(theme.bg_dark)
                            .with_selection_range(Some(EXECUTE_STDOUT_RANGE_BASE))
                            .with_joiner(joiner),
                    );
                }
            }
        }

        BlockOutput { lines }
    }
}

/// Drop a leading `Run` / `Running` word (case-insensitive) plus following
/// whitespace so Label headers do not read `Run Run the tests`.
fn strip_leading_run_word(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let rest = if let Some(rest) = lower.strip_prefix("running") {
        rest
    } else if let Some(rest) = lower.strip_prefix("run") {
        rest
    } else {
        return s.to_string();
    };
    // Require a word boundary after Run/Running (space or end).
    if rest.is_empty() {
        return String::new();
    }
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return s.to_string();
    }
    // Map back to original casing via byte length of the prefix consumed
    // (`to_ascii_lowercase` preserves length for ASCII prefixes).
    let prefix_len = s.len() - rest.len();
    s[prefix_len..].trim_start().to_string()
}

impl BlockContent for ExecuteToolCallBlock {
    fn output(&self, ctx: &BlockContext) -> BlockOutput {
        let theme = Theme::current();
        let config = &ctx.appearance.scrollback.blocks.execute;
        let header_style = config.header_style;

        // Content width already nets out the bullet; hang indent is only for
        // `$` / `Run` prefix (not the bullet again — that double-counted).
        let content_width = ctx.content_width();

        match ctx.mode {
            DisplayMode::Collapsed => {
                let muted = ctx.mute_when_collapsed(config.muted_command_collapsed);
                let mut lines = Vec::new();
                // Collapsed: description title only (no `$ command`) for density;
                // without description, still show the single-line command header.
                self.push_header_lines(
                    &mut lines,
                    &theme,
                    header_style,
                    muted,
                    content_width,
                    0,
                    true,
                    false, // hide command when description is the title
                );
                BlockOutput { lines }
            }
            DisplayMode::Truncated => self.render_with_truncation(
                &theme,
                content_width,
                Some((config.first_lines as usize, config.last_lines as usize)),
                header_style,
                0,
            ),
            DisplayMode::Expanded => {
                self.render_with_truncation(&theme, content_width, None, header_style, 0)
            }
        }
    }

    fn accent(&self, ctx: &BlockContext) -> Option<AccentStyle> {
        if !ctx.appearance.scrollback.blocks.execute.accent_enabled {
            return None;
        }
        let theme = Theme::current();
        if self.error.is_some() {
            Some(AccentStyle::static_color(theme.accent_error))
        } else if ctx.is_running {
            Some(AccentStyle::animated(
                ctx.appearance.scrollback.blocks.execute.running_accent,
            ))
        } else {
            Some(AccentStyle::static_color(theme.accent_success))
        }
    }

    fn has_vpad(&self, _ctx: &BlockContext) -> bool {
        false
    }

    fn background(&self, _ctx: &BlockContext) -> BlockBackground {
        BlockBackground::None
    }

    fn has_raw_mode(&self) -> bool {
        false
    }

    fn is_foldable(&self) -> bool {
        // Collapsed with a description hides `$ command`; expand reveals it
        // (and output/error when present). Use Label-style stripping so a bare
        // "Run"/"Running" description (stripped to empty) does not claim a
        // fold when collapsed and expanded headers are identical.
        self.description_display(true).is_some() || self.output.is_some() || self.error.is_some()
    }

    /// Fold cycle. Agent tools toggle Collapsed <-> Truncated (glanceable
    /// preview; full output in the viewer). User `!` commands toggle
    /// Collapsed <-> Expanded: re-expanding must restore the full output,
    /// never the lossy first/last window.
    fn next_fold_mode(&self, current: DisplayMode, _is_running: bool) -> DisplayMode {
        match current {
            DisplayMode::Collapsed if self.bash_mode => DisplayMode::Expanded,
            DisplayMode::Collapsed => DisplayMode::Truncated,
            DisplayMode::Truncated | DisplayMode::Expanded => DisplayMode::Collapsed,
        }
    }

    /// Minimum fold mode used by collapse + the running expand chevron.
    ///
    /// Agent tools default to Collapsed (title only) — no auto-expand. User
    /// `!` bash while running uses Truncated so interactive output streams and
    /// the chevron treats that as min-fold. When finished, Collapsed is always
    /// the true minimum (user can fold to title-only).
    fn collapse_mode(&self, is_running: bool) -> DisplayMode {
        if self.bash_mode && is_running {
            DisplayMode::Truncated
        } else {
            DisplayMode::Collapsed
        }
    }

    /// Agent tools start **Collapsed** (no auto-expand of stdout). User `!`
    /// bash starts Truncated so output streams — errors included: the
    /// tracker's Completed refinement resets to this default right before
    /// `finish_running`, so a Collapsed error default would defeat the
    /// Expanded finish.
    fn default_display_mode(&self) -> DisplayMode {
        if self.bash_mode {
            DisplayMode::Truncated
        } else {
            DisplayMode::Collapsed
        }
    }

    fn finished_display_mode(&self) -> Option<DisplayMode> {
        if self.bash_mode {
            // Interactive bash: full output on finish, like a terminal —
            // the streaming preview would silently drop the middle lines.
            Some(DisplayMode::Expanded)
        } else {
            // Agent tools: do **not** force a mode on finish.
            // - Never auto-expanded at start (default Collapsed).
            // - If the user manually expanded while running, keep that mode
            //   (no snap-shut). If they left it collapsed, it stays collapsed.
            None
        }
    }

    fn preamble(&self, ctx: &BlockContext) -> Option<Text<'static>> {
        let theme = Theme::current();
        let header_style = ctx.appearance.scrollback.blocks.execute.header_style;

        let mut lines: Vec<Line<'static>> = self
            .header_lines(&theme, header_style, false, true)
            .into_iter()
            .map(|(line, _)| line)
            .collect();

        if self.output.is_none()
            && let Some(error) = &self.error
            && !error.is_empty()
        {
            lines.push(Line::from(""));
            let error_style = ratatui::style::Style::default().fg(theme.accent_error);
            for line in error.lines() {
                lines.push(Line::from(Span::styled(line.to_string(), error_style)));
            }
        }

        Some(Text::from(lines))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::ExecuteHeaderStyle;

    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn header_line_uses_header_display_when_set_command_stays_full() {
        let mut block = ExecuteToolCallBlock::new("cd /proj && echo hi");
        block.header_display = Some("echo hi".into());
        assert_eq!(block.command, "cd /proj && echo hi");
        let theme = Theme::current();
        let headers = block.header_lines(&theme, ExecuteHeaderStyle::Label, false, true);
        assert_eq!(headers.len(), 1);
        let text = line_text(&headers[0].0);
        assert!(text.contains("echo hi"), "header={text:?}");
        assert!(
            !text.contains("cd /proj"),
            "header must use header_display: {text:?}"
        );
    }

    #[test]
    fn label_header_with_description_shows_title_then_command() {
        let block = ExecuteToolCallBlock::new("cargo test --lib")
            .with_description("Run the unit test suite");
        let theme = Theme::current();
        let headers = block.header_lines(&theme, ExecuteHeaderStyle::Label, false, true);
        assert_eq!(headers.len(), 2);
        let title = line_text(&headers[0].0);
        let cmd = line_text(&headers[1].0);
        // Leading "Run " on the description is stripped (Label already has it).
        assert_eq!(title, "Run the unit test suite");
        assert!(cmd.starts_with("$ "), "cmd={cmd:?}");
        assert!(cmd.contains("cargo test --lib"), "cmd={cmd:?}");
        // Prefix span counts: "Run " only on title; "$ " on command.
        assert_eq!(headers[0].1, 1);
        assert_eq!(headers[1].1, 1);
    }

    #[test]
    fn collapsed_header_with_description_hides_command() {
        let block = ExecuteToolCallBlock::new("cargo test --lib")
            .with_description("Run the unit test suite");
        let theme = Theme::current();
        let headers = block.header_lines(&theme, ExecuteHeaderStyle::Label, true, false);
        assert_eq!(headers.len(), 1);
        assert_eq!(line_text(&headers[0].0), "Run the unit test suite");
    }

    #[test]
    fn strip_leading_run_word_handles_run_and_running() {
        assert_eq!(strip_leading_run_word("Run the tests"), "the tests");
        assert_eq!(strip_leading_run_word("running checks"), "checks");
        assert_eq!(strip_leading_run_word("runtime config"), "runtime config");
        assert_eq!(strip_leading_run_word("Check status"), "Check status");
    }

    #[test]
    fn shell_header_with_description_shows_title_then_command() {
        let block =
            ExecuteToolCallBlock::new("git status -sb").with_description("Check git status");
        let theme = Theme::current();
        let headers = block.header_lines(&theme, ExecuteHeaderStyle::Shell, false, true);
        assert_eq!(headers.len(), 2);
        assert_eq!(line_text(&headers[0].0), "Check git status");
        assert_eq!(headers[0].1, 0);
        let cmd = line_text(&headers[1].0);
        assert!(cmd.starts_with("$ "), "cmd={cmd:?}");
        assert!(cmd.contains("git status -sb"), "cmd={cmd:?}");
    }

    #[test]
    fn label_header_without_description_is_single_run_command_line() {
        let block = ExecuteToolCallBlock::new("echo hi");
        let theme = Theme::current();
        let headers = block.header_lines(&theme, ExecuteHeaderStyle::Label, false, false);
        assert_eq!(headers.len(), 1);
        let text = line_text(&headers[0].0);
        assert!(text.starts_with("Run "), "header={text:?}");
        assert!(text.contains("echo hi"), "header={text:?}");
    }

    #[test]
    fn label_header_lines_flattens_command_newlines() {
        // Single-line header path must never embed raw `\n` (ratatui drops them).
        let block = ExecuteToolCallBlock::new("cargo test \\\n  --all");
        let theme = Theme::current();
        let headers = block.header_lines(&theme, ExecuteHeaderStyle::Label, false, true);
        assert_eq!(headers.len(), 1);
        let text = line_text(&headers[0].0);
        assert!(
            !text.contains('\n'),
            "label single-line header must flatten newlines: {text:?}"
        );
        assert!(text.contains("cargo test"), "header={text:?}");
        assert!(text.contains("--all"), "header={text:?}");
    }

    #[test]
    fn label_expanded_soft_wraps_multiline_command_like_permission_panel() {
        let block = ExecuteToolCallBlock::new(
            "git status --short --branch && cargo test --workspace --all-features",
        );
        let theme = Theme::current();
        let mut lines = Vec::new();
        block.push_header_lines(
            &mut lines,
            &theme,
            ExecuteHeaderStyle::Label,
            false,
            40,
            0,
            false, // expanded: soft-wrap
            true,
        );
        assert!(
            lines.len() >= 2,
            "expected operator soft-wrap rows, got {}",
            lines.len()
        );
        let first = line_text(&lines[0].content);
        assert!(
            first.starts_with("Run "),
            "Label soft-wrap first row needs Run prefix: {first:?}"
        );
        assert!(
            first.contains("&&"),
            "first row should keep the operator: {first:?}"
        );
        assert!(
            !first.contains('\n'),
            "each BlockLine is one visual row: {first:?}"
        );
        let second = line_text(&lines[1].content);
        assert!(
            second.trim_start().starts_with("cargo"),
            "continuation under hang: {second:?}"
        );
        assert_eq!(
            lines[1].joiner.as_deref(),
            Some("\n"),
            "copy must preserve line breaks between soft-wrap rows"
        );
    }

    #[test]
    fn empty_description_treated_as_absent() {
        let block = ExecuteToolCallBlock::new("echo hi").with_description("   \n  ");
        let theme = Theme::current();
        let headers = block.header_lines(&theme, ExecuteHeaderStyle::Label, false, false);
        assert_eq!(headers.len(), 1);
        assert!(line_text(&headers[0].0).contains("echo hi"));
    }

    #[test]
    fn label_bash_mode_with_description_keeps_user_marker_on_title() {
        let mut block = ExecuteToolCallBlock::new("ls -la").with_description("List files");
        block.bash_mode = true;
        let theme = Theme::current();
        let headers = block.header_lines(&theme, ExecuteHeaderStyle::Label, false, true);
        assert_eq!(headers.len(), 2);
        let title = line_text(&headers[0].0);
        assert_eq!(title, "Run (user) List files");
        assert_eq!(headers[0].1, 2); // "Run " + "(user) "
    }

    #[test]
    fn description_makes_block_foldable_to_reveal_command() {
        let block = ExecuteToolCallBlock::new("true").with_description("no-op");
        assert!(block.is_foldable());
        let with_output = block.clone().with_output("ok\n");
        assert!(with_output.is_foldable());
        let no_desc = ExecuteToolCallBlock::new("true");
        assert!(!no_desc.is_foldable());
    }

    #[test]
    fn agent_execute_does_not_auto_expand_and_preserves_fold_on_finish() {
        let agent = ExecuteToolCallBlock::new("echo hi").with_description("say hi");
        // No auto-expand: start collapsed (title only).
        assert_eq!(agent.default_display_mode(), DisplayMode::Collapsed);
        assert_eq!(agent.collapse_mode(true), DisplayMode::Collapsed);
        assert_eq!(agent.collapse_mode(false), DisplayMode::Collapsed);
        // Finish does not force open or shut — preserve user choice (None).
        assert_eq!(agent.finished_display_mode(), None);

        let mut bash = ExecuteToolCallBlock::new("ls");
        bash.bash_mode = true;
        // User ! streams truncated, finishes with the full output.
        assert_eq!(bash.default_display_mode(), DisplayMode::Truncated);
        assert_eq!(bash.finished_display_mode(), Some(DisplayMode::Expanded));
        assert_eq!(bash.collapse_mode(true), DisplayMode::Truncated);
        assert_eq!(bash.collapse_mode(false), DisplayMode::Collapsed);
        // Fold cycle for user bash skips the lossy Truncated window.
        assert_eq!(
            bash.next_fold_mode(DisplayMode::Collapsed, false),
            DisplayMode::Expanded
        );
        assert_eq!(
            bash.next_fold_mode(DisplayMode::Expanded, false),
            DisplayMode::Collapsed
        );

        let failed = ExecuteToolCallBlock::new("false").with_error("exit 1");
        assert_eq!(failed.default_display_mode(), DisplayMode::Collapsed);
        assert_eq!(failed.finished_display_mode(), None);

        // Failed user bash keeps the Truncated default: a Collapsed default
        // would defeat the finish expand (see default_display_mode).
        let mut failed_bash = ExecuteToolCallBlock::new("pytest").with_error("exit 2");
        failed_bash.bash_mode = true;
        assert_eq!(failed_bash.default_display_mode(), DisplayMode::Truncated);
        assert_eq!(
            failed_bash.finished_display_mode(),
            Some(DisplayMode::Expanded)
        );
    }

    #[test]
    fn bare_run_description_is_not_foldable() {
        let bare = ExecuteToolCallBlock::new("echo hi").with_description("Run");
        assert!(
            !bare.is_foldable(),
            "Label strips bare Run; headers identical → not foldable"
        );
        let real = ExecuteToolCallBlock::new("echo hi").with_description("say hi");
        assert!(real.is_foldable());
    }

    #[test]
    fn test_push_output_basic() {
        let mut block = ExecuteToolCallBlock::new("echo test");
        block.push_output("hello ");
        block.push_output("world");
        block.finish();
        assert_eq!(block.output, Some("hello world".to_string()));
    }

    #[test]
    fn test_push_output_multiple_chunks() {
        let mut block = ExecuteToolCallBlock::new("cargo build");
        block.push_output("Com");
        block.push_output("pili");
        block.push_output("ng ");
        block.push_output("crat");
        block.push_output("e\n");
        block.finish();
        assert_eq!(block.output, Some("Compiling crate\n".to_string()));
    }

    #[test]
    fn push_output_guardrail_collapses_and_keeps_head_tail() {
        let mut block = ExecuteToolCallBlock::new("cmd");
        let head = "h".repeat(EXECUTE_OUTPUT_KEEP_HEAD_BYTES);
        let body = "m".repeat(EXECUTE_OUTPUT_MAX_BYTES);
        let tail = "t".repeat(EXECUTE_OUTPUT_KEEP_TAIL_BYTES);
        block.push_output(&head);
        block.push_output(&body);
        block.push_output(&tail);
        let out = block.output.as_deref().expect("output present");
        assert!(out.starts_with('h'));
        assert!(out.ends_with('t'));
        assert!(out.contains("omitted in memory"));
        assert!(out.len() < EXECUTE_OUTPUT_KEEP_HEAD_BYTES + EXECUTE_OUTPUT_KEEP_TAIL_BYTES + 256);
    }

    #[test]
    fn push_output_guardrail_keeps_tail_window_rolling() {
        let mut block = ExecuteToolCallBlock::new("cmd");
        let big = "x".repeat(EXECUTE_OUTPUT_MAX_BYTES + 1);
        block.push_output(&big);
        assert!(
            block
                .output
                .as_deref()
                .unwrap()
                .contains("omitted in memory")
        );
        // Continue streaming after the guardrail trips: the tail window must
        // stay bounded while still tracking the latest bytes.
        let more = "y".repeat(EXECUTE_OUTPUT_KEEP_TAIL_BYTES * 2);
        block.push_output(&more);
        let out = block.output.as_deref().unwrap();
        assert!(out.ends_with('y'));
        assert!(out.len() < EXECUTE_OUTPUT_KEEP_HEAD_BYTES + EXECUTE_OUTPUT_KEEP_TAIL_BYTES + 512);
    }

    #[test]
    fn replace_output_guardrail_applies_for_full_buffer_path() {
        let mut block = ExecuteToolCallBlock::new("cmd");
        let big = "z".repeat(EXECUTE_OUTPUT_MAX_BYTES + 10);
        block.replace_output(&big);
        let out = block.output.as_deref().unwrap();
        assert!(out.contains("omitted in memory"));
        assert!(out.starts_with('z'));
        assert!(out.ends_with('z'));
        assert!(out.len() < EXECUTE_OUTPUT_MAX_BYTES / 2);
        // Small outputs pass through untouched.
        block.replace_output("small");
        assert_eq!(block.output.as_deref(), Some("small"));
    }

    #[test]
    fn test_with_output() {
        let block = ExecuteToolCallBlock::new("echo test").with_output("plain text output");
        assert_eq!(block.output, Some("plain text output".to_string()));
    }
}
