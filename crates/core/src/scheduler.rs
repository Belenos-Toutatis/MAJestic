//! Scheduled background check through the Windows Task Scheduler.
//!
//! The task runs `MAJestic.exe --check` as the current user (no elevation),
//! catches up when the PC was off at the scheduled time, and only runs with a
//! network connection. It is registered from an XML definition because
//! `schtasks` flags cannot express "run as soon as possible after a missed
//! start".

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{Datelike, Local};

use crate::exec;
use crate::settings::{Frequency, Schedule};

pub const TASK_NAME: &str = "MAJestic UpdateCheck";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn valid_time(t: &str) -> String {
    let mut parts = t.split(':');
    let h = parts.next().and_then(|h| h.trim().parse::<u32>().ok()).filter(|h| *h < 24).unwrap_or(12);
    let m = parts.next().and_then(|m| m.trim().parse::<u32>().ok()).filter(|m| *m < 60).unwrap_or(0);
    format!("{h:02}:{m:02}")
}

/// Task definition for `schedule`.
pub fn task_xml(schedule: &Schedule, exe: &Path, user: &str) -> String {
    let today = Local::now();
    let start = format!("{}T{}:00", today.format("%Y-%m-%d"), valid_time(&schedule.time));
    let trigger = match schedule.frequency {
        Frequency::Daily => "<ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay>".to_string(),
        Frequency::Weekly => {
            let day = match today.weekday() {
                chrono::Weekday::Mon => "Monday",
                chrono::Weekday::Tue => "Tuesday",
                chrono::Weekday::Wed => "Wednesday",
                chrono::Weekday::Thu => "Thursday",
                chrono::Weekday::Fri => "Friday",
                chrono::Weekday::Sat => "Saturday",
                chrono::Weekday::Sun => "Sunday",
            };
            format!("<ScheduleByWeek><DaysOfWeek><{day} /></DaysOfWeek><WeeksInterval>1</WeeksInterval></ScheduleByWeek>")
        }
        Frequency::Monthly => {
            let months = [
                "January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
                "November", "December",
            ]
            .iter()
            .map(|m| format!("<{m} />"))
            .collect::<String>();
            format!(
                "<ScheduleByMonth><DaysOfMonth><Day>{}</Day></DaysOfMonth><Months>{months}</Months></ScheduleByMonth>",
                today.day().min(28)
            )
        }
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>MAJestic: check for software updates</Description>
  </RegistrationInfo>
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>{start}</StartBoundary>
      <Enabled>true</Enabled>
      {trigger}
    </CalendarTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>true</RunOnlyIfNetworkAvailable>
    <ExecutionTimeLimit>PT1H</ExecutionTimeLimit>
    <Enabled>true</Enabled>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>--check</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        user = xml_escape(user),
        exe = xml_escape(&exe.to_string_lossy()),
    )
}

fn current_user() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    match std::env::var("USERDOMAIN") {
        Ok(domain) if !domain.is_empty() => format!("{domain}\\{user}"),
        _ => user,
    }
}

/// Creates/updates (or deletes when `None`) the scheduled task running
/// `exe --check`.
pub fn apply(schedule: Option<&Schedule>, exe: &Path) -> anyhow::Result<()> {
    let timeout = Duration::from_secs(30);
    let Some(schedule) = schedule else {
        // Deleting a task that does not exist is not an error for us.
        let _ = exec::run("schtasks", &["/Delete", "/TN", TASK_NAME, "/F"], timeout);
        return Ok(());
    };

    // schtasks wants the XML file in UTF-16 LE with a BOM.
    let xml = task_xml(schedule, exe, &current_user());
    let mut bytes = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let file = std::env::temp_dir().join(format!("majestic-task-{}.xml", std::process::id()));
    std::fs::write(&file, bytes)?;
    let path = file.to_string_lossy().into_owned();
    let out = exec::run("schtasks", &["/Create", "/TN", TASK_NAME, "/XML", &path, "/F"], timeout);
    let _ = std::fs::remove_file(&file);
    let out = out?;
    if out.code != 0 {
        anyhow::bail!("schtasks failed ({}): {} {}", out.code, out.stdout.trim(), out.stderr.trim());
    }
    Ok(())
}

/// What Windows has registered under [`TASK_NAME`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskState {
    /// No such task (never created, or removed by hand or by an antivirus).
    Missing,
    /// The task runs this program.
    Runs(PathBuf),
    /// The task exists but its definition could not be read.
    Unreadable,
}

/// Reads the registered task (`schtasks /Query /XML`), without changing it.
pub fn registered_task() -> anyhow::Result<TaskState> {
    let out = exec::run("schtasks", &["/Query", "/TN", TASK_NAME, "/XML"], Duration::from_secs(30))?;
    if out.code != 0 {
        return Ok(TaskState::Missing);
    }
    Ok(command_of(&out.stdout).map_or(TaskState::Unreadable, TaskState::Runs))
}

fn command_of(xml: &str) -> Option<PathBuf> {
    let start = xml.find("<Command>")? + "<Command>".len();
    let end = start + xml[start..].find("</Command>")?;
    let raw = xml[start..end].trim();
    let unescaped = raw.replace("&quot;", "\"").replace("&gt;", ">").replace("&lt;", "<").replace("&amp;", "&");
    Some(PathBuf::from(unescaped.trim_matches('"')))
}

/// Same executable path (case and quotes ignored).
pub fn same_command(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().trim().trim_matches('"').replace('/', "\\").to_lowercase();
    norm(a) == norm(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_command_parsing() {
        let xml = "<?xml version=\"1.0\"?>\r\n<Task><Actions><Exec>\r\n<Command>C:\\Tools\\A&amp;B\\MAJestic.exe</Command>\r\n\
                   <Arguments>--check</Arguments></Exec></Actions></Task>";
        assert_eq!(command_of(xml), Some(PathBuf::from(r"C:\Tools\A&B\MAJestic.exe")));
        assert_eq!(command_of("<Task></Task>"), None);
        assert!(same_command(Path::new(r"C:\Users\Me\Downloads\MAJestic.exe"), Path::new(r#""c:\users\me\downloads\majestic.exe""#)));
        assert!(!same_command(Path::new(r"C:\A\MAJestic.exe"), Path::new(r"C:\B\MAJestic.exe")));
    }

    #[test]
    fn xml_contains_command_and_trigger() {
        let s = Schedule { frequency: Frequency::Daily, time: "9:5".into(), notify: Default::default() };
        let xml = task_xml(&s, Path::new(r"C:\Tools\A&B\MAJestic.exe"), "PC\\me");
        assert!(xml.contains(r"<Command>C:\Tools\A&amp;B\MAJestic.exe</Command>"));
        assert!(xml.contains("T09:05:00</StartBoundary>"));
        assert!(xml.contains("<DaysInterval>1</DaysInterval>"));
        assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
    }

    #[test]
    fn time_is_sanitized() {
        assert_eq!(valid_time("25:99"), "12:00");
        assert_eq!(valid_time("07:30"), "07:30");
    }
}
