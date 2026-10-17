//! The two settings of a unit file that decide whether a unit is the real one: what it unsets from
//! the environment and what it runs. Pure text in, values out.

/// What the `[Service]` sections of a unit file and its drop-ins say, folded in order.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ServiceSettings {
    /// The names `UnsetEnvironment=` removes.
    pub unset: Vec<String>,
    /// The program of the first `ExecStart=`, as written.
    pub exec: Option<String>,
}

impl ServiceSettings {
    /// These settings after reading `text`, the next file in precedence order. An empty
    /// assignment starts the list over, as systemd does.
    pub fn fold(mut self, text: &str) -> Self {
        let mut in_service = false;
        for line in logical_lines(text) {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                in_service = section == "Service";
                continue;
            }
            if !in_service {
                continue;
            }
            match line.split_once('=') {
                Some(("UnsetEnvironment", value)) => self.fold_unset(value.trim()),
                Some(("ExecStart", value)) => self.fold_exec(value.trim()),
                _ => {}
            }
        }
        self
    }

    fn fold_unset(&mut self, value: &str) {
        if value.is_empty() {
            self.unset.clear();
            return;
        }
        self.unset.extend(
            value
                .split_whitespace()
                .map(|word| word.trim_matches(['"', '\'']).to_owned()),
        );
    }

    fn fold_exec(&mut self, value: &str) {
        if value.is_empty() {
            self.exec = None;
        } else if self.exec.is_none() {
            self.exec = program_of(value);
        }
    }
}

/// The program of an `ExecStart=` value: the first word after the prefix characters.
fn program_of(value: &str) -> Option<String> {
    let rest = value.trim_start_matches(['@', '-', ':', '+', '!']);
    rest.split_whitespace().next().map(str::to_owned)
}

/// The lines of `text` with a trailing backslash joined to the next line.
fn logical_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut open = String::new();
    for raw in text.lines() {
        match raw.strip_suffix('\\') {
            Some(head) => {
                open.push_str(head);
                open.push(' ');
            }
            None => {
                open.push_str(raw);
                lines.push(std::mem::take(&mut open));
            }
        }
    }
    if !open.is_empty() {
        lines.push(open);
    }
    lines
}
