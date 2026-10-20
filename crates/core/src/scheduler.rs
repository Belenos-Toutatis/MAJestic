//! Scheduled background check through the Windows Task Scheduler.
//!
//! The task runs `MAJestic.exe --check` as the current user (no elevation),
//! catches up when the PC was off at the scheduled time, and only runs with a
//! network connection. It is registered from an XML definition because
//! `schtasks` flags cannot express "run as soon as possible after a missed
//! start".

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::exec;
use crate::settings::{Frequency, Schedule};

pub const TASK_NAME: &str = "MAJestic UpdateCheck";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn hour_minute(t: &str) -> (u32, u32) {
    let mut parts = t.split(':');
    let h = parts.next().and_then(|h| h.trim().parse::<u32>().ok()).filter(|h| *h < 24).unwrap_or(12);
    let m = parts.next().and_then(|m| m.trim().parse::<u32>().ok()).filter(|m| *m < 60).unwrap_or(0);
    (h, m)
}

fn valid_time(t: &str) -> String {
    let (h, m) = hour_minute(t);
    format!("{h:02}:{m:02}")
}

const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/// Weekday of a weekly schedule, 1 = Monday … 7 = Sunday (today when unset).
fn weekday_of(schedule: &Schedule, today: NaiveDate) -> u32 {
    schedule.weekday.map(u32::from).filter(|w| (1..=7).contains(w)).unwrap_or_else(|| today.weekday().number_from_monday())
}

/// Day of a monthly schedule, 1–28 (today, at most 28, when unset).
fn day_of(schedule: &Schedule, today: NaiveDate) -> u32 {
    schedule.day.map(u32::from).filter(|d| (1..=28).contains(d)).unwrap_or_else(|| today.day().min(28))
}

/// Next time the task will run after `now`, for a schedule created with the
/// weekday/day it holds. `None` when the schedule does not tell (older
/// settings without them).
pub fn next_run(schedule: &Schedule, now: NaiveDateTime) -> Option<NaiveDateTime> {
    let (h, m) = hour_minute(&schedule.time);
    let time = NaiveTime::from_hms_opt(h, m, 0)?;
    let today = now.date();
    match schedule.frequency {
        Frequency::Daily => {
            let at = today.and_time(time);
            Some(if at > now { at } else { at + chrono::Duration::days(1) })
        }
        Frequency::Weekly => {
            let wanted = u32::from(schedule.weekday?);
            (0..8)
                .map(|k| today + chrono::Duration::days(k))
                .filter(|d| d.weekday().number_from_monday() == wanted)
                .map(|d| d.and_time(time))
                .find(|at| *at > now)
        }
        Frequency::Monthly => {
            let day = u32::from(schedule.day?).clamp(1, 28);
            (0..3).find_map(|k| {
                let months = today.year() * 12 + today.month0() as i32 + k;
                let date = NaiveDate::from_ymd_opt(months.div_euclid(12), months.rem_euclid(12) as u32 + 1, day)?;
                Some(date.and_time(time)).filter(|at| *at > now)
            })
        }
    }
}

/// Task definition for `schedule`.
pub fn task_xml(schedule: &Schedule, exe: &Path, user: &str) -> String {
    let today = Local::now();
    let start = format!("{}T{}:00", today.format("%Y-%m-%d"), valid_time(&schedule.time));
    let trigger = match schedule.frequency {
        Frequency::Daily => "<ScheduleByDay><DaysInterval>1</DaysInterval></ScheduleByDay>".to_string(),
        Frequency::Weekly => {
            let day = WEEKDAYS[weekday_of(schedule, today.date_naive()) as usize - 1];
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
                day_of(schedule, today.date_naive())
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

/// Result of the last background check (`--check`), shown in the settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastCheck {
    /// RFC 3339, local time.
    pub at: String,
    pub updates: usize,
    pub important: usize,
    /// Security updates waiting in Windows Update.
    #[serde(default)]
    pub windows_security: usize,
    /// A notification was shown.
    pub notified: bool,
}

fn last_check_file() -> PathBuf {
    crate::paths::data_dir().join("last-check.json")
}

pub fn save_last_check(check: &LastCheck) -> anyhow::Result<()> {
    std::fs::create_dir_all(crate::paths::data_dir())?;
    std::fs::write(last_check_file(), serde_json::to_vec_pretty(check)?)?;
    Ok(())
}

pub fn load_last_check() -> Option<LastCheck> {
    serde_json::from_slice(&std::fs::read(last_check_file()).ok()?).ok()
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
        let s = Schedule { frequency: Frequency::Daily, time: "9:5".into(), notify: Default::default(), ..Default::default() };
        let xml = task_xml(&s, Path::new(r"C:\Tools\A&B\MAJestic.exe"), "PC\\me");
        assert!(xml.contains(r"<Command>C:\Tools\A&amp;B\MAJestic.exe</Command>"));
        assert!(xml.contains("T09:05:00</StartBoundary>"));
        assert!(xml.contains("<DaysInterval>1</DaysInterval>"));
        assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
    }

    #[test]
    fn chosen_day_and_next_run() {
        let at = |s: &str| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        let sched = |frequency, weekday, day| Schedule { frequency, time: "12:00".into(), weekday, day, ..Default::default() };
        // 2026-10-20 is a Tuesday.
        let now = at("2026-10-20 13:00");
        assert_eq!(next_run(&sched(Frequency::Daily, None, None), now), Some(at("2026-10-21 12:00")));
        assert_eq!(next_run(&sched(Frequency::Daily, None, None), at("2026-10-20 08:00")), Some(at("2026-10-20 12:00")));
        assert_eq!(next_run(&sched(Frequency::Weekly, Some(1), None), now), Some(at("2026-10-26 12:00")));
        assert_eq!(next_run(&sched(Frequency::Weekly, Some(2), None), now), Some(at("2026-10-27 12:00")));
        assert_eq!(next_run(&sched(Frequency::Weekly, Some(2), None), at("2026-10-20 11:00")), Some(at("2026-10-20 12:00")));
        assert_eq!(next_run(&sched(Frequency::Monthly, None, Some(5)), now), Some(at("2026-11-05 12:00")));
        assert_eq!(next_run(&sched(Frequency::Monthly, None, Some(5)), at("2026-12-10 09:00")), Some(at("2027-01-05 12:00")));
        assert_eq!(next_run(&sched(Frequency::Weekly, None, None), now), None);

        let xml = task_xml(&sched(Frequency::Weekly, Some(5), None), Path::new(r"C:\M.exe"), "me");
        assert!(xml.contains("<DaysOfWeek><Friday /></DaysOfWeek>"));
        let xml = task_xml(&sched(Frequency::Monthly, None, Some(17)), Path::new(r"C:\M.exe"), "me");
        assert!(xml.contains("<Day>17</Day>"));
    }

    #[test]
    fn time_is_sanitized() {
        assert_eq!(valid_time("25:99"), "12:00");
        assert_eq!(valid_time("07:30"), "07:30");
    }
}
