use super::{
    AVATAR_MAX, CEILING, Conversation, LOCK_WAIT, MAX_SAFE_INTEGER, Parsed, Problem, Reading,
    Refused, Row, bots_file, composed_home, entries_of, is_absolute_path, is_identifier, is_model,
    is_slug, is_text, list_waiting, lock, make_state_directory, move_with, now, parse_entries,
    write_entries,
};
use crate::agents::{self, Made, MakeRefused, RedefineRefused};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SLUG_CUT: usize = 48;

#[derive(Debug, Clone, PartialEq)]
pub struct NewBot {
    pub name: String,
    pub purpose: String,
    pub model: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Changes {
    pub name: Option<String>,
    pub purpose: Option<String>,
    pub model: Option<Option<String>>,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Saved {
    pub row: Row,
    pub home_made: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Noted {
    pub row: Option<Row>,
    pub wrote: bool,
}

pub fn slug_for(name: &str, taken: &HashSet<&str>) -> String {
    let mut base = String::new();
    for character in name.to_lowercase().chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            base.push(character);
        } else if !base.is_empty() && !base.ends_with('-') {
            base.push('-');
        }
    }
    base.truncate(SLUG_CUT);
    base.truncate(base.trim_end_matches('-').len());
    if base.is_empty() {
        base.push_str("bot");
    }
    if !taken.contains(base.as_str()) {
        return base;
    }
    (2..)
        .map(|number| format!("{base}-{number}"))
        .find(|tried| !taken.contains(tried.as_str()))
        .unwrap_or(base)
}

pub fn make(data_folder: &Path, new: NewBot) -> Result<Saved, Refused> {
    let home = writable_home()?;
    make_in(&home, data_folder, new, now(), LOCK_WAIT)
}

pub fn make_in(
    home: &Path,
    data_folder: &Path,
    new: NewBot,
    now: f64,
    wait: Duration,
) -> Result<Saved, Refused> {
    make_with(home, data_folder, new, now, wait, agents::make_definition)
}

fn make_with(
    home: &Path,
    data_folder: &Path,
    new: NewBot,
    now: f64,
    wait: Duration,
    define: impl FnOnce(&Path, &str, &str, Option<&str>) -> Result<Made, MakeRefused>,
) -> Result<Saved, Refused> {
    validate(
        Some(&new.name),
        Some(&new.purpose),
        Some(new.model.as_deref()),
        new.avatar.as_deref(),
    )?;
    require_text_data_folder(data_folder)?;
    let row = modify(home, data_folder, now, wait, |rows| {
        if rows.len() >= CEILING {
            return Err(Refused::Ceiling);
        }
        let slug = {
            let taken = rows.iter().map(|row| row.slug.as_str()).collect();
            slug_for(&new.name, &taken)
        };
        let folder = composed_home(data_folder, &slug)
            .ok_or_else(|| Refused::DataFolder(data_folder.to_path_buf()))?;
        let made = define(home, &slug, &new.purpose, new.model.as_deref())
            .map_err(|refused| make_refusal(home, refused))?;
        if !is_slug(&made.name) {
            return Err(Refused::Definition(format!(
                "the agent named the definition {:?}, which is not a slug",
                made.name
            )));
        }
        let row = Row {
            avatar: new.avatar.unwrap_or_else(|| slug.clone()),
            slug,
            name: new.name,
            purpose: new.purpose,
            model: new.model,
            home: folder,
            definition: Some(made.name),
            session: None,
            conversations: Vec::new(),
            archived: 0,
            retired: 0.0,
            created: now,
            updated: now,
        };
        rows.push(row.clone());
        Ok(row)
    })?;
    let home_made = make_home(&row.home);
    Ok(Saved { row, home_made })
}

pub fn edit(data_folder: &Path, slug: &str, changes: Changes) -> Result<Saved, Refused> {
    let home = writable_home()?;
    edit_in(&home, data_folder, slug, changes, now(), LOCK_WAIT)
}

pub fn edit_in(
    home: &Path,
    data_folder: &Path,
    slug: &str,
    changes: Changes,
    now: f64,
    wait: Duration,
) -> Result<Saved, Refused> {
    validate(
        changes.name.as_deref(),
        changes.purpose.as_deref(),
        changes.model.as_ref().map(Option::as_deref),
        changes.avatar.as_deref(),
    )?;
    require_text_data_folder(data_folder)?;
    let row = change_row(home, data_folder, slug, now, wait, |row| {
        if let Some(name) = changes.name {
            row.name = name;
        }
        if let Some(purpose) = changes.purpose {
            row.purpose = purpose;
        }
        if let Some(model) = changes.model {
            row.model = model;
        }
        if let Some(avatar) = changes.avatar {
            row.avatar = avatar;
        }
    })?;
    let home_made = make_home(&row.home);
    Ok(Saved { row, home_made })
}

pub fn set_model(data_folder: &Path, slug: &str, model: Option<&str>) -> Result<Row, Refused> {
    let home = writable_home()?;
    set_model_in(&home, data_folder, slug, model, now(), LOCK_WAIT)
}

pub fn set_model_in(
    home: &Path,
    data_folder: &Path,
    slug: &str,
    model: Option<&str>,
    now: f64,
    wait: Duration,
) -> Result<Row, Refused> {
    validate(None, None, Some(model), None)?;
    require_text_data_folder(data_folder)?;
    change_row(home, data_folder, slug, now, wait, |row| {
        row.model = model.map(str::to_string);
    })
}

pub fn ensure_home(data_folder: &Path, slug: &str) -> Result<bool, Refused> {
    let home = writable_home()?;
    ensure_home_in(&home, data_folder, slug, now(), LOCK_WAIT)
}

pub fn ensure_home_in(
    home: &Path,
    data_folder: &Path,
    slug: &str,
    now: f64,
    wait: Duration,
) -> Result<bool, Refused> {
    require_text_data_folder(data_folder)?;
    let listing = list_waiting(home, true, data_folder, now, wait);
    if let Some(problem) = listing.unreadable {
        return Err(Refused::Unreadable(problem));
    }
    let row = listing
        .rows
        .iter()
        .find(|row| row.slug == slug)
        .ok_or_else(|| Refused::NoSuchBot(slug.to_string()))?;
    Ok(make_home(&row.home))
}

pub fn archive(data_folder: &Path, slug: &str) -> Result<Row, Refused> {
    let home = writable_home()?;
    archive_in(&home, data_folder, slug, now(), LOCK_WAIT)
}

pub fn archive_in(
    home: &Path,
    data_folder: &Path,
    slug: &str,
    now: f64,
    wait: Duration,
) -> Result<Row, Refused> {
    require_text_data_folder(data_folder)?;
    stamp_retired(home, data_folder, slug, now, now, wait)
}

pub fn restore(data_folder: &Path, slug: &str) -> Result<Row, Refused> {
    let home = writable_home()?;
    restore_in(&home, data_folder, slug, now(), LOCK_WAIT)
}

pub fn restore_in(
    home: &Path,
    data_folder: &Path,
    slug: &str,
    now: f64,
    wait: Duration,
) -> Result<Row, Refused> {
    require_text_data_folder(data_folder)?;
    stamp_retired(home, data_folder, slug, 0.0, now, wait)
}

pub fn delete(data_folder: &Path, slug: &str) -> Result<(), Refused> {
    if !is_slug(slug) {
        return Err(Refused::NoSuchBot(slug.to_string()));
    }
    let home = writable_home()?;
    delete_in(&home, data_folder, slug, now(), LOCK_WAIT)
}

pub fn delete_in(
    home: &Path,
    data_folder: &Path,
    slug: &str,
    now: f64,
    wait: Duration,
) -> Result<(), Refused> {
    if !is_slug(slug) {
        return Err(Refused::NoSuchBot(slug.to_string()));
    }
    require_text_data_folder(data_folder)?;
    modify_keeping(
        home,
        data_folder,
        now,
        wait,
        |rows| {
            let index = rows
                .iter()
                .position(|row| row.slug == slug)
                .ok_or_else(|| Refused::NoSuchBot(slug.to_string()))?;
            rows.remove(index);
            Ok(data_folder.join("bots").join(slug))
        },
        |folder| remove_history(folder),
    )
    .map(|_| ())
}

pub fn record_conversation(
    data_folder: &Path,
    slug: &str,
    id: &str,
    directory: &str,
) -> Result<Noted, Refused> {
    let home = writable_home()?;
    record_conversation_in(&home, data_folder, slug, id, directory, now(), LOCK_WAIT)
}

pub fn record_conversation_in(
    home: &Path,
    data_folder: &Path,
    slug: &str,
    id: &str,
    directory: &str,
    now: f64,
    wait: Duration,
) -> Result<Noted, Refused> {
    require_text_data_folder(data_folder)?;
    amend(home, data_folder, now, wait, |rows| {
        let Some(held) = rows.iter_mut().find(|row| row.slug == slug) else {
            return Ok((quiet(None), false));
        };
        if !is_identifier(id) || !is_absolute_path(directory) {
            return Err(Refused::Conversation);
        }
        held.session = Some(id.to_string());
        if !held
            .conversations
            .iter()
            .any(|each| each.id == id && each.directory == directory)
        {
            held.conversations.push(Conversation {
                id: id.to_string(),
                directory: directory.to_string(),
            });
        }
        held.updated = now;
        Ok((
            Noted {
                row: Some(held.clone()),
                wrote: true,
            },
            true,
        ))
    })
}

pub fn record_archived(data_folder: &Path, slug: &str, archived: f64) -> Result<Noted, Refused> {
    let home = writable_home()?;
    record_archived_in(&home, data_folder, slug, archived, now(), LOCK_WAIT)
}

pub fn record_archived_in(
    home: &Path,
    data_folder: &Path,
    slug: &str,
    archived: f64,
    now: f64,
    wait: Duration,
) -> Result<Noted, Refused> {
    require_text_data_folder(data_folder)?;
    amend(home, data_folder, now, wait, |rows| {
        let Some(held) = rows.iter_mut().find(|row| row.slug == slug) else {
            return Ok((quiet(None), false));
        };
        let Some(next) = whole_archived(archived) else {
            return Ok((quiet(Some(held.clone())), false));
        };
        if next <= held.archived {
            return Ok((quiet(Some(held.clone())), false));
        }
        held.archived = next;
        held.updated = now;
        Ok((
            Noted {
                row: Some(held.clone()),
                wrote: true,
            },
            true,
        ))
    })
}

pub fn release_session(data_folder: &Path, slug: &str) -> Result<Noted, Refused> {
    let home = writable_home()?;
    release_session_in(&home, data_folder, slug, now(), LOCK_WAIT)
}

pub fn release_session_in(
    home: &Path,
    data_folder: &Path,
    slug: &str,
    now: f64,
    wait: Duration,
) -> Result<Noted, Refused> {
    require_text_data_folder(data_folder)?;
    amend(home, data_folder, now, wait, |rows| {
        let Some(held) = rows.iter_mut().find(|row| row.slug == slug) else {
            return Ok((quiet(None), false));
        };
        if held.session.is_none() {
            return Ok((quiet(Some(held.clone())), false));
        }
        held.session = None;
        held.archived = 0;
        held.updated = now;
        Ok((
            Noted {
                row: Some(held.clone()),
                wrote: true,
            },
            true,
        ))
    })
}

fn quiet(row: Option<Row>) -> Noted {
    Noted { row, wrote: false }
}

pub fn forget_conversation(
    data_folder: &Path,
    id: &str,
    directory: &str,
) -> Result<usize, Refused> {
    let home = writable_home()?;
    forget_conversation_in(&home, data_folder, id, directory, now(), LOCK_WAIT)
}

pub fn forget_conversation_in(
    home: &Path,
    data_folder: &Path,
    id: &str,
    directory: &str,
    now: f64,
    wait: Duration,
) -> Result<usize, Refused> {
    require_text_data_folder(data_folder)?;
    amend(home, data_folder, now, wait, |rows| {
        let mut changed = 0;
        for held in rows.iter_mut() {
            let listed = held.conversations.len();
            held.conversations
                .retain(|each| !(each.id == id && each.directory == directory));
            let removed = held.conversations.len() != listed;
            let pointed = held.session.as_deref() == Some(id);
            if !removed && !pointed {
                continue;
            }
            if pointed {
                held.session = None;
                held.archived = 0;
            }
            held.updated = now;
            changed += 1;
        }
        Ok((changed, changed > 0))
    })
}

fn whole_archived(value: f64) -> Option<u64> {
    if value.fract() == 0.0 && (0.0..=MAX_SAFE_INTEGER).contains(&value) {
        Some(value as u64)
    } else {
        None
    }
}

fn stamp_retired(
    home: &Path,
    data_folder: &Path,
    slug: &str,
    retired: f64,
    now: f64,
    wait: Duration,
) -> Result<Row, Refused> {
    modify(home, data_folder, now, wait, |rows| {
        let held = rows
            .iter_mut()
            .find(|row| row.slug == slug)
            .ok_or_else(|| Refused::NoSuchBot(slug.to_string()))?;
        held.retired = retired;
        held.updated = now;
        Ok(held.clone())
    })
}

fn remove_history(folder: &Path) -> Result<(), Refused> {
    match std::fs::remove_dir_all(folder) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Refused::History(Problem::new(folder, error))),
    }
}

fn require_text_data_folder(data_folder: &Path) -> Result<(), Refused> {
    match composed_home(data_folder, "bot") {
        Some(_) => Ok(()),
        None => Err(Refused::DataFolder(data_folder.to_path_buf())),
    }
}

fn writable_home() -> Result<PathBuf, Refused> {
    if bravebot_core::incognito::engaged() {
        return Err(Refused::Incognito);
    }
    crate::home::writable().ok_or(Refused::NoFolder)
}

fn validate(
    name: Option<&str>,
    purpose: Option<&str>,
    model: Option<Option<&str>>,
    avatar: Option<&str>,
) -> Result<(), Refused> {
    if name.is_some_and(|name| !is_text(name)) {
        return Err(Refused::Name);
    }
    if purpose.is_some_and(|purpose| !is_text(purpose)) {
        return Err(Refused::Purpose);
    }
    if model.flatten().is_some_and(|model| !is_model(model)) {
        return Err(Refused::Model);
    }
    if avatar.is_some_and(|avatar| !is_text(avatar) || avatar.chars().count() > AVATAR_MAX) {
        return Err(Refused::Avatar);
    }
    Ok(())
}

fn modify<T>(
    home: &Path,
    data_folder: &Path,
    now: f64,
    wait: Duration,
    change: impl FnOnce(&mut Vec<Row>) -> Result<T, Refused>,
) -> Result<T, Refused> {
    modify_keeping(home, data_folder, now, wait, change, |_: &T| Ok(()))
}

fn modify_keeping<T>(
    home: &Path,
    data_folder: &Path,
    now: f64,
    wait: Duration,
    change: impl FnOnce(&mut Vec<Row>) -> Result<T, Refused>,
    and_then: impl FnOnce(&T) -> Result<(), Refused>,
) -> Result<T, Refused> {
    revise(
        home,
        data_folder,
        now,
        wait,
        |rows| change(rows).map(|value| (value, true)),
        and_then,
    )
}

fn amend<T>(
    home: &Path,
    data_folder: &Path,
    now: f64,
    wait: Duration,
    change: impl FnOnce(&mut Vec<Row>) -> Result<(T, bool), Refused>,
) -> Result<T, Refused> {
    revise(home, data_folder, now, wait, change, |_| Ok(()))
}

fn revise<T>(
    home: &Path,
    data_folder: &Path,
    now: f64,
    wait: Duration,
    change: impl FnOnce(&mut Vec<Row>) -> Result<(T, bool), Refused>,
    and_then: impl FnOnce(&T) -> Result<(), Refused>,
) -> Result<T, Refused> {
    move_with(home, data_folder, wait, || {})?;
    make_state_directory(home)?;
    let _held = lock(home, wait)?;
    let path = bots_file(home);
    let mut rows = match std::fs::read_to_string(&path) {
        Ok(text) => {
            let entries = entries_of(&text)
                .map_err(|reason| Refused::Unreadable(Problem::new(&path, reason)))?;
            let Parsed { rows, .. } = parse_entries(&entries, &Reading { data_folder, now });
            rows
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(Refused::Unreadable(Problem::new(&path, error))),
    };
    let (outcome, changed) = change(&mut rows)?;
    if !changed {
        return Ok(outcome);
    }
    if rows.len() > CEILING {
        return Err(Refused::Ceiling);
    }
    rows.sort_by(|a, b| a.slug.cmp(&b.slug));
    and_then(&outcome)?;
    write_entries(&path, rows.iter().map(Row::to_value).collect())?;
    Ok(outcome)
}

fn change_row(
    home: &Path,
    data_folder: &Path,
    slug: &str,
    now: f64,
    wait: Duration,
    change: impl FnOnce(&mut Row),
) -> Result<Row, Refused> {
    modify(home, data_folder, now, wait, |rows| {
        let held = rows
            .iter_mut()
            .find(|row| row.slug == slug)
            .ok_or_else(|| Refused::NoSuchBot(slug.to_string()))?;
        let mut changed = held.clone();
        change(&mut changed);
        if let Some(definition) = &changed.definition {
            agents::redefine(home, definition, &changed.purpose, changed.model.as_deref())
                .map_err(|refused| redefine_refusal(home, definition, refused))?;
        }
        changed.updated = now;
        *held = changed.clone();
        Ok(changed)
    })
}

fn make_refusal(home: &Path, refused: MakeRefused) -> Refused {
    match refused {
        MakeRefused::Name => {
            Refused::Definition("the slug is not one a definition can be named".into())
        }
        MakeRefused::Model => {
            Refused::Definition("the model must be one line with nothing around it".into())
        }
        MakeRefused::Purpose => {
            Refused::Definition("the purpose needs a line that is not blank".into())
        }
        MakeRefused::Io(error) => Refused::DefinitionIo(Problem::new(&home.join("agents"), error)),
    }
}

fn redefine_refusal(home: &Path, definition: &str, refused: RedefineRefused) -> Refused {
    match refused {
        RedefineRefused::Name => {
            Refused::Definition(format!("{definition} is not a name a definition can have"))
        }
        RedefineRefused::Missing => {
            Refused::Definition(format!("the definition {definition} is not there to edit"))
        }
        RedefineRefused::Refused(_) => Refused::Definition(
            "the purpose needs a line that is not blank, and the model must be one line".into(),
        ),
        RedefineRefused::Io(error) => Refused::DefinitionIo(Problem::new(
            &home.join("agents").join(format!("{definition}.md")),
            error,
        )),
    }
}

fn make_home(folder: &str) -> bool {
    if !Path::new(folder).is_absolute() {
        return false;
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(folder).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const NOW: f64 = 1_000.0;
    const WAIT: Duration = Duration::from_secs(5);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(&format!("bots-operations-{name}"));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }

        fn home(&self) -> PathBuf {
            self.0.join("state")
        }

        fn data(&self) -> PathBuf {
            self.0.join("gui")
        }

        fn bots_text(&self) -> Option<String> {
            std::fs::read_to_string(self.home().join("bots.json")).ok()
        }

        fn stored(&self) -> Vec<Value> {
            entries_of(&self.bots_text().expect("a bots file")).expect("a bots array")
        }

        fn write_bots(&self, entries: &[Value]) {
            std::fs::create_dir_all(self.home()).expect("a state directory");
            std::fs::write(
                self.home().join("bots.json"),
                json!({ "bots": entries }).to_string(),
            )
            .expect("a bots file");
        }

        fn entry(&self, slug: &str) -> Value {
            json!({
                "slug": slug, "name": slug, "purpose": "Does a thing.", "avatar": slug,
                "model": null,
                "home": self.data().join("bot-homes").join(slug).to_str().unwrap(),
                "definition": null, "session": null, "conversations": [],
                "archived": 0, "retired": 0, "created": 5, "updated": 6,
            })
        }

        fn definition(&self, name: &str) -> Option<String> {
            std::fs::read_to_string(self.home().join("agents").join(format!("{name}.md"))).ok()
        }

        fn make(&self, name: &str) -> Saved {
            make_in(&self.home(), &self.data(), new_bot(name), NOW, WAIT).expect("made")
        }

        fn row(&self, slug: &str) -> Row {
            super::super::list_in(&self.home(), true, &self.data(), NOW)
                .rows
                .into_iter()
                .find(|row| row.slug == slug)
                .expect("the row")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn new_bot(name: &str) -> NewBot {
        NewBot {
            name: name.to_string(),
            purpose: "Reviews the change.\nThen reports.".to_string(),
            model: None,
            avatar: None,
        }
    }

    fn slug_of(name: &str, taken: &[&str]) -> String {
        slug_for(name, &taken.iter().copied().collect())
    }

    #[test]
    fn a_slug_is_composed_from_the_name_as_the_spec_says() {
        let long_break = format!("{} {}", "a".repeat(47), "b".repeat(10));
        let cases = [
            ("Web Dev", "web-dev"),
            ("  Hello,   World!!  ", "hello-world"),
            ("--x", "x"),
            ("___a__b___", "a-b"),
            ("UPPER9", "upper9"),
            ("Café Au Lait", "caf-au-lait"),
            ("", "bot"),
            ("!!!", "bot"),
            ("ÅÄÖ", "bot"),
            (&"x".repeat(60), &"x".repeat(48)),
            (&long_break, &"a".repeat(47)),
            (
                &format!("{}  b", "a".repeat(46)),
                &format!("{}-b", "a".repeat(46)),
            ),
        ];
        for (name, slug) in cases {
            assert_eq!(slug_of(name, &[]), slug, "{name:?}");
        }
    }

    #[test]
    fn a_slug_another_row_holds_takes_a_number() {
        assert_eq!(slug_of("Dev", &["dev"]), "dev-2");
        assert_eq!(slug_of("Dev", &["dev", "dev-2"]), "dev-3");
        assert_eq!(slug_of("Dev", &["dev-2"]), "dev");
        assert_eq!(slug_of("", &["bot"]), "bot-2");
        let cut = "a".repeat(48);
        assert_eq!(slug_of(&"a".repeat(80), &[&cut]), format!("{cut}-2"));
    }

    #[test]
    fn a_composed_slug_is_always_a_slug() {
        let names = [
            "",
            " ",
            "-",
            "--a--",
            "\0",
            "a\0b",
            "\u{85}",
            "İstanbul",
            "ǅ",
            "日本語",
            "emoji \u{1f600} bot",
            "a/b\\c.d",
            &"-".repeat(100),
            &format!("{}-{}", "a".repeat(47), "b"),
            &format!("{}{}", "a".repeat(48), "-b"),
        ];
        for name in names {
            let slug = slug_of(name, &[]);
            assert!(is_slug(&slug), "{name:?} gave {slug:?}");
            let taken = slug_of(name, &[&slug]);
            assert!(is_slug(&taken), "{name:?} gave {taken:?}");
        }
    }

    #[test]
    fn a_bot_is_made_with_its_definition_its_row_and_its_home() {
        let scratch = Scratch::new("make");
        let mut new = new_bot("Code Reviewer");
        new.model = Some("provider/model-a".to_string());

        let saved = make_in(&scratch.home(), &scratch.data(), new, NOW, WAIT).unwrap();

        let row = &saved.row;
        assert_eq!(row.slug, "code-reviewer");
        assert_eq!(row.name, "Code Reviewer");
        assert_eq!(row.avatar, "code-reviewer");
        assert_eq!(row.model.as_deref(), Some("provider/model-a"));
        assert_eq!(row.definition.as_deref(), Some("code-reviewer"));
        assert_eq!(
            row.home,
            scratch
                .data()
                .join("bot-homes/code-reviewer")
                .to_str()
                .unwrap()
        );
        assert_eq!(
            (row.session.as_deref(), row.archived, row.retired),
            (None, 0, 0.0)
        );
        assert!(row.conversations.is_empty());
        assert_eq!((row.created, row.updated), (NOW, NOW));
        assert!(saved.home_made);
        assert!(Path::new(&row.home).is_dir());
        assert_eq!(scratch.stored(), vec![row.to_value()]);
        let definition = scratch.definition("code-reviewer").expect("a definition");
        assert!(
            definition.contains("Reviews the change.\nThen reports."),
            "{definition}"
        );
        assert!(
            definition.contains("model: 'provider/model-a'"),
            "{definition}"
        );
    }

    #[test]
    fn an_avatar_that_is_given_is_kept() {
        let scratch = Scratch::new("avatar");
        let mut new = new_bot("Dev");
        new.avatar = Some("seed-7".to_string());

        let saved = make_in(&scratch.home(), &scratch.data(), new, NOW, WAIT).unwrap();

        assert_eq!(saved.row.avatar, "seed-7");
    }

    #[test]
    fn a_taken_name_is_numbered_and_each_bot_has_a_definition_of_its_own() {
        let scratch = Scratch::new("numbered");
        let mut archived = scratch.entry("dev");
        archived["retired"] = json!(900);
        scratch.write_bots(&[archived]);

        let second = scratch.make("Dev");
        let third = scratch.make("Dev");

        assert_eq!(second.row.slug, "dev-2");
        assert_eq!(third.row.slug, "dev-3");
        assert_eq!(second.row.definition.as_deref(), Some("dev-2"));
        assert_eq!(third.row.definition.as_deref(), Some("dev-3"));
        assert!(scratch.definition("dev-2").is_some());
        assert!(scratch.definition("dev-3").is_some());
        let slugs: Vec<_> = scratch.stored().iter().map(|e| e["slug"].clone()).collect();
        assert_eq!(slugs, [json!("dev"), json!("dev-2"), json!("dev-3")]);
    }

    #[test]
    fn rows_come_out_in_the_byte_order_of_the_slug() {
        let scratch = Scratch::new("order");
        for name in ["b", "a2", "a-1", "10", "a"] {
            scratch.make(name);
        }

        let slugs: Vec<_> = scratch
            .stored()
            .iter()
            .map(|entry| entry["slug"].as_str().unwrap().to_string())
            .collect();

        assert_eq!(slugs, ["10", "a", "a-1", "a2", "b"]);
    }

    #[test]
    fn a_form_that_breaks_a_bound_is_refused_and_nothing_is_written() {
        let scratch = Scratch::new("bounds");
        let with = |change: &dyn Fn(&mut NewBot)| {
            let mut new = new_bot("Ok");
            change(&mut new);
            new
        };
        let cases: Vec<(NewBot, &str)> = vec![
            (with(&|n| n.name = String::new()), "Name"),
            (with(&|n| n.name = " \t\u{a0}".into()), "Name"),
            (with(&|n| n.name = "a\0b".into()), "Name"),
            (with(&|n| n.purpose = "  ".into()), "Purpose"),
            (with(&|n| n.purpose = "a\0b".into()), "Purpose"),
            (with(&|n| n.model = Some("m".repeat(513))), "Model"),
            (with(&|n| n.model = Some("m\nn".into())), "Model"),
            (with(&|n| n.model = Some("inherit".into())), "Model"),
            (with(&|n| n.model = Some(" m".into())), "Model"),
            (with(&|n| n.model = Some(String::new())), "Model"),
            (with(&|n| n.avatar = Some(" ".into())), "Avatar"),
            (with(&|n| n.avatar = Some("a".repeat(129))), "Avatar"),
            (with(&|n| n.avatar = Some("a\0".into())), "Avatar"),
        ];
        for (new, expected) in cases {
            let refused =
                make_in(&scratch.home(), &scratch.data(), new.clone(), NOW, WAIT).unwrap_err();
            assert_eq!(format!("{refused:?}"), expected, "{new:?}");
        }
        assert!(!scratch.home().exists());
        assert!(!scratch.data().exists());
    }

    #[test]
    fn a_model_and_an_avatar_at_their_bounds_are_accepted() {
        let scratch = Scratch::new("at-bounds");
        let mut new = new_bot("Edge");
        new.model = Some("m".repeat(512));
        new.avatar = Some("a".repeat(128));

        assert!(make_in(&scratch.home(), &scratch.data(), new, NOW, WAIT).is_ok());
    }

    #[test]
    fn an_opaque_model_identifier_with_slashes_and_a_colon_is_accepted_everywhere_a_model_is_taken()
    {
        const OPAQUE: &str = "openrouter/provider/model:free";
        let scratch = Scratch::new("opaque-model");
        let mut new = new_bot("Dev");
        new.model = Some(OPAQUE.to_string());

        let made = make_in(&scratch.home(), &scratch.data(), new, NOW, WAIT).unwrap();

        assert_eq!(made.row.model.as_deref(), Some(OPAQUE));
        assert_eq!(scratch.row("dev").model.as_deref(), Some(OPAQUE));
        let definition = scratch.definition("dev").expect("a definition");
        assert!(
            definition.contains(&format!("model: '{OPAQUE}'")),
            "{definition}"
        );

        let set = set_model_in(
            &scratch.home(),
            &scratch.data(),
            "dev",
            Some("custom-model"),
            NOW,
            WAIT,
        )
        .unwrap();
        assert_eq!(set.model.as_deref(), Some("custom-model"));
        let cleared =
            set_model_in(&scratch.home(), &scratch.data(), "dev", None, NOW, WAIT).unwrap();
        assert_eq!(cleared.model, None);
        assert_eq!(scratch.row("dev").model, None);

        let edited = edit_in(
            &scratch.home(),
            &scratch.data(),
            "dev",
            Changes {
                model: Some(Some(OPAQUE.into())),
                ..Changes::default()
            },
            NOW,
            WAIT,
        )
        .unwrap();
        assert_eq!(edited.row.model.as_deref(), Some(OPAQUE));
        assert_eq!(scratch.row("dev").model.as_deref(), Some(OPAQUE));
    }

    #[test]
    fn a_data_folder_that_cannot_be_written_into_a_row_refuses_the_make() {
        let scratch = Scratch::new("data-folder");
        let mut folders = vec![PathBuf::from("relative/gui")];
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let invalid = std::ffi::OsStr::from_bytes(b"/gui-\xff");
            folders.push(PathBuf::from(invalid));
        }
        for folder in folders {
            let refused = make_in(&scratch.home(), &folder, new_bot("Dev"), NOW, WAIT).unwrap_err();
            match refused {
                Refused::DataFolder(named) => assert_eq!(named, folder),
                other => panic!("{other}"),
            }
        }
        assert!(!scratch.home().exists());
    }

    #[test]
    fn a_definition_the_agent_refuses_saves_no_row() {
        let scratch = Scratch::new("refused-definition");
        scratch.write_bots(&[scratch.entry("kept")]);
        let before = scratch.bots_text();
        let mut new = new_bot("Dev");
        new.purpose = "\u{85}".to_string();

        let refused = make_in(&scratch.home(), &scratch.data(), new, NOW, WAIT).unwrap_err();

        assert!(matches!(refused, Refused::Definition(_)), "{refused}");
        assert_eq!(scratch.bots_text(), before);
        assert!(!scratch.home().join("agents").exists());
        assert!(!scratch.data().join("bot-homes").exists());
    }

    #[test]
    fn a_definition_name_that_is_not_a_slug_saves_no_row() {
        let scratch = Scratch::new("bad-name");
        let define = |home: &Path, _: &str, _: &str, _: Option<&str>| {
            Ok(Made {
                name: "Not A Slug".to_string(),
                file: home.join("agents/x.md"),
            })
        };

        let refused = make_with(
            &scratch.home(),
            &scratch.data(),
            new_bot("Dev"),
            NOW,
            WAIT,
            define,
        )
        .unwrap_err();

        assert!(matches!(refused, Refused::Definition(_)), "{refused}");
        assert_eq!(scratch.bots_text(), None);
        assert!(!scratch.data().join("bot-homes").exists());
    }

    #[test]
    fn a_definition_that_cannot_be_written_saves_no_row() {
        let scratch = Scratch::new("definition-io");
        std::fs::create_dir_all(scratch.home()).unwrap();
        std::fs::write(
            scratch.home().join("agents"),
            "a file where the folder belongs",
        )
        .unwrap();

        let refused =
            make_in(&scratch.home(), &scratch.data(), new_bot("Dev"), NOW, WAIT).unwrap_err();

        match refused {
            Refused::DefinitionIo(problem) => {
                assert_eq!(problem.path, scratch.home().join("agents"));
            }
            other => panic!("{other}"),
        }
        assert_eq!(scratch.bots_text(), None);
        assert!(!scratch.data().join("bot-homes").exists());
    }

    #[test]
    fn the_hundred_and_first_bot_is_refused_and_no_definition_is_written() {
        let scratch = Scratch::new("ceiling");
        let mut entries: Vec<Value> = (0..100)
            .map(|n| scratch.entry(&format!("b{n:03}")))
            .collect();
        entries[0]["retired"] = json!(5);
        scratch.write_bots(&entries);
        let before = scratch.bots_text();

        let refused = make_in(
            &scratch.home(),
            &scratch.data(),
            new_bot("One Too Many"),
            NOW,
            WAIT,
        )
        .unwrap_err();

        assert!(matches!(refused, Refused::Ceiling), "{refused}");
        assert_eq!(scratch.bots_text(), before);
        assert!(!scratch.home().join("agents").exists());
        assert!(!scratch.data().join("bot-homes").exists());
    }

    #[test]
    fn the_hundredth_bot_is_made() {
        let scratch = Scratch::new("hundredth");
        let entries: Vec<Value> = (0..99)
            .map(|n| scratch.entry(&format!("b{n:03}")))
            .collect();
        scratch.write_bots(&entries);

        scratch.make("Last One");

        assert_eq!(scratch.stored().len(), 100);
    }

    #[cfg(unix)]
    #[test]
    fn the_first_make_creates_the_state_directory_the_file_the_data_folder_and_the_home_privately()
    {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new("first-run");

        let saved = scratch.make("First");

        let mode = |path: PathBuf| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(scratch.home()), 0o700);
        assert_eq!(mode(scratch.home().join("bots.json")), 0o600);
        assert_eq!(mode(scratch.data()), 0o700);
        assert_eq!(mode(scratch.data().join("bot-homes")), 0o700);
        assert_eq!(mode(PathBuf::from(&saved.row.home)), 0o700);
        assert!(saved.home_made);
    }

    #[cfg(unix)]
    #[test]
    fn a_row_saved_while_its_home_cannot_be_made_is_kept_and_says_so() {
        let scratch = Scratch::new("home-blocked");
        std::fs::create_dir_all(scratch.data()).unwrap();
        std::fs::write(scratch.data().join("bot-homes"), "a file").unwrap();

        let saved = scratch.make("Dev");

        assert!(!saved.home_made);
        assert_eq!(scratch.stored(), vec![saved.row.to_value()]);
    }

    #[test]
    fn a_make_is_refused_naming_the_old_file_while_it_cannot_be_converted() {
        let scratch = Scratch::new("old-file");
        std::fs::create_dir_all(scratch.data()).unwrap();
        let old = scratch.data().join("bravebot-ui.json");
        std::fs::write(&old, "{\"bots\": [").unwrap();

        let refused =
            make_in(&scratch.home(), &scratch.data(), new_bot("Dev"), NOW, WAIT).unwrap_err();

        match refused {
            Refused::OldFile(problem) => assert_eq!(problem.path, old),
            other => panic!("{other}"),
        }
        assert_eq!(scratch.bots_text(), None);
        assert!(!scratch.home().join("agents").exists());
        assert_eq!(std::fs::read_to_string(old).unwrap(), "{\"bots\": [");
    }

    #[test]
    fn a_make_after_the_rows_moved_from_the_old_file_keeps_them() {
        let scratch = Scratch::new("old-rows");
        std::fs::create_dir_all(scratch.data()).unwrap();
        std::fs::write(
            scratch.data().join("bravebot-ui.json"),
            json!({"bots": [scratch.entry("old")]}).to_string(),
        )
        .unwrap();

        scratch.make("New");

        let slugs: Vec<_> = scratch.stored().iter().map(|e| e["slug"].clone()).collect();
        assert_eq!(slugs, [json!("new"), json!("old")]);
    }

    #[test]
    fn a_file_that_does_not_parse_is_never_overwritten() {
        for (name, text) in [
            ("truncated", "{\"bots\": [{\"slug\": \"a\""),
            ("empty", ""),
            ("not-bots", "{\"bots\": 4}"),
        ] {
            let scratch = Scratch::new(&format!("unparsed-{name}"));
            std::fs::create_dir_all(scratch.home()).unwrap();
            std::fs::write(scratch.home().join("bots.json"), text).unwrap();

            let made = make_in(&scratch.home(), &scratch.data(), new_bot("Dev"), NOW, WAIT);
            let edited = edit_in(
                &scratch.home(),
                &scratch.data(),
                "dev",
                Changes::default(),
                NOW,
                WAIT,
            );
            let modeled = set_model_in(&scratch.home(), &scratch.data(), "dev", None, NOW, WAIT);

            for refused in [made.map(|_| ()), edited.map(|_| ()), modeled.map(|_| ())] {
                assert!(
                    matches!(refused, Err(Refused::Unreadable(_))),
                    "{name}: {refused:?}"
                );
            }
            assert_eq!(scratch.bots_text().as_deref(), Some(text), "{name}");
            assert!(!scratch.home().join("agents").exists(), "{name}");
        }
    }

    #[test]
    fn a_row_the_read_drops_is_removed_by_the_write() {
        let scratch = Scratch::new("dropped");
        let mut blank = scratch.entry("blank");
        blank["name"] = json!(" ");
        scratch.write_bots(&[scratch.entry("kept"), blank, json!("junk")]);

        scratch.make("New");

        let slugs: Vec<_> = scratch.stored().iter().map(|e| e["slug"].clone()).collect();
        assert_eq!(slugs, [json!("kept"), json!("new")]);
    }

    #[cfg(unix)]
    #[test]
    fn a_data_folder_that_cannot_be_text_refuses_an_edit_a_model_change_and_an_ensure_before_any_row_is_dropped()
     {
        use std::os::unix::ffi::OsStrExt;
        let scratch = Scratch::new("data-folder-edit");
        let mut legacy = scratch.entry("legacy");
        legacy.as_object_mut().unwrap().remove("home");
        scratch.write_bots(&[legacy, scratch.entry("keep")]);
        let before = scratch.bots_text();
        let folder = PathBuf::from(std::ffi::OsStr::from_bytes(b"/gui-\xff"));

        let refusals = [
            edit_in(
                &scratch.home(),
                &folder,
                "keep",
                Changes {
                    name: Some("K".into()),
                    ..Changes::default()
                },
                NOW,
                WAIT,
            )
            .map(|_| ()),
            set_model_in(&scratch.home(), &folder, "keep", Some("m"), NOW, WAIT).map(|_| ()),
            ensure_home_in(&scratch.home(), &folder, "keep", NOW, WAIT).map(|_| ()),
        ];

        for refused in refusals {
            match refused {
                Err(Refused::DataFolder(named)) => assert_eq!(named, folder),
                other => panic!("not refused for the data folder: {other:?}"),
            }
        }
        assert_eq!(scratch.bots_text(), before);
        assert!(!scratch.home().join("bots.json.lock").exists());
    }

    #[cfg(unix)]
    fn state_is_read_only(scratch: &Scratch) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(scratch.home(), std::fs::Permissions::from_mode(0o500)).unwrap();
        let probe = scratch.home().join("probe");
        if std::fs::write(&probe, "").is_ok() {
            std::fs::set_permissions(scratch.home(), std::fs::Permissions::from_mode(0o700))
                .unwrap();
            eprintln!("SKIPPED (permissions do not bind this user)");
            return false;
        }
        true
    }

    #[cfg(unix)]
    fn state_is_writable_again(scratch: &Scratch) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(scratch.home(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_definition_is_written_first_and_stays_when_the_row_cannot_be_saved() {
        let scratch = Scratch::new("definition-first");
        scratch.make("First");
        let before = scratch.bots_text();
        if !state_is_read_only(&scratch) {
            return;
        }
        let mut new = new_bot("Second");
        new.purpose = "Audits the second thing.".to_string();

        let refused = make_in(&scratch.home(), &scratch.data(), new, NOW, WAIT);
        state_is_writable_again(&scratch);

        assert!(matches!(refused, Err(Refused::Io(_))), "{refused:?}");
        assert_eq!(scratch.bots_text(), before);
        let definition = scratch
            .definition("second")
            .expect("written before the row");
        assert!(
            definition.contains("Audits the second thing."),
            "{definition}"
        );
        assert!(!scratch.data().join("bot-homes/second").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_write_that_fails_before_the_rename_leaves_the_file_as_it_was() {
        let scratch = Scratch::new("interrupted");
        scratch.write_bots(&[scratch.entry("old")]);
        let before = scratch.bots_text();
        std::fs::write(scratch.home().join("bots.json.lock"), "").unwrap();
        if !state_is_read_only(&scratch) {
            return;
        }

        let refused = edit_in(
            &scratch.home(),
            &scratch.data(),
            "old",
            Changes {
                name: Some("Renamed".into()),
                ..Changes::default()
            },
            NOW,
            WAIT,
        );
        state_is_writable_again(&scratch);

        match refused {
            Err(Refused::Io(problem)) => {
                assert_eq!(problem.path, scratch.home().join("bots.json"));
            }
            other => panic!("not refused at the write: {other:?}"),
        }
        assert_eq!(scratch.bots_text(), before);
        let leftovers: Vec<_> = std::fs::read_dir(scratch.home())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn two_writers_at_once_lose_no_row() {
        let scratch = Scratch::new("racing");
        let home = scratch.home();
        let data = scratch.data();
        let names: Vec<String> = (0..8).map(|n| format!("Writer {n}")).collect();

        std::thread::scope(|scope| {
            for name in &names {
                let (home, data) = (&home, &data);
                scope.spawn(move || {
                    make_in(home, data, new_bot(name), NOW, Duration::from_secs(30))
                        .expect("made while others write");
                });
            }
        });

        assert_eq!(scratch.stored().len(), 8);
        for n in 0..8 {
            assert!(scratch.definition(&format!("writer-{n}")).is_some(), "{n}");
        }
    }

    #[test]
    fn a_lock_a_stopped_program_left_behind_is_not_held() {
        let scratch = Scratch::new("stale-lock");
        std::fs::create_dir_all(scratch.home()).unwrap();
        std::fs::write(scratch.home().join("bots.json.lock"), "").unwrap();

        let saved = make_in(
            &scratch.home(),
            &scratch.data(),
            new_bot("Dev"),
            NOW,
            Duration::from_millis(100),
        );

        assert!(saved.is_ok(), "{saved:?}");
    }

    #[test]
    fn a_lock_held_past_the_wait_refuses_every_write_naming_the_lock_and_writes_nothing() {
        let scratch = Scratch::new("held");
        scratch.write_bots(&[scratch.entry("old")]);
        let before = scratch.bots_text();
        let held = lock(&scratch.home(), Duration::from_secs(1)).unwrap();
        let wait = Duration::from_millis(100);
        let (home, data) = (scratch.home(), scratch.data());

        let refusals = [
            make_in(&home, &data, new_bot("Dev"), NOW, wait).map(|_| ()),
            edit_in(&home, &data, "old", Changes::default(), NOW, wait).map(|_| ()),
            set_model_in(&home, &data, "old", Some("m"), NOW, wait).map(|_| ()),
        ];
        drop(held);

        for refused in refusals {
            match refused {
                Err(Refused::Locked(path)) => assert_eq!(path, home.join("bots.json.lock")),
                other => panic!("not refused for the lock: {other:?}"),
            }
        }
        assert_eq!(scratch.bots_text(), before);
        assert!(!home.join("agents").exists());
    }

    #[test]
    fn a_lock_held_past_the_wait_leaves_a_definition_as_it_was_for_an_edit_and_a_model_change() {
        let scratch = Scratch::new("held-definition");
        scratch.make("Dev");
        let rows = scratch.bots_text();
        let definition = scratch.definition("dev");
        let held = lock(&scratch.home(), Duration::from_secs(1)).unwrap();
        let wait = Duration::from_millis(5);
        let (home, data) = (scratch.home(), scratch.data());

        let refusals = [
            edit_in(
                &home,
                &data,
                "dev",
                Changes {
                    purpose: Some("Audits something else.".into()),
                    ..Changes::default()
                },
                NOW,
                wait,
            )
            .map(|_| ()),
            set_model_in(&home, &data, "dev", Some("model-z"), NOW, wait).map(|_| ()),
        ];
        drop(held);

        for refused in refusals {
            match refused {
                Err(Refused::Locked(path)) => assert_eq!(path, home.join("bots.json.lock")),
                other => panic!("not refused for the lock: {other:?}"),
            }
        }
        assert_eq!(scratch.definition("dev"), definition);
        assert_eq!(scratch.bots_text(), rows);
    }

    fn made_with_memory(scratch: &Scratch) -> Row {
        let mut new = new_bot("Editor");
        new.model = Some("model-a".to_string());
        make_in(&scratch.home(), &scratch.data(), new, NOW, WAIT).unwrap();
        let mut entries = scratch.stored();
        entries[0]["session"] = json!("s-1");
        entries[0]["conversations"] = json!([{"id": "s-1", "directory": "/projects/p"}]);
        entries[0]["archived"] = json!(4);
        entries[0]["retired"] = json!(700);
        scratch.write_bots(&entries);
        scratch.row("editor")
    }

    #[test]
    fn an_edit_rewrites_the_definition_and_keeps_what_happened() {
        let scratch = Scratch::new("edit");
        let before = made_with_memory(&scratch);

        let saved = edit_in(
            &scratch.home(),
            &scratch.data(),
            "editor",
            Changes {
                name: Some("Proofreader".into()),
                purpose: Some("Proofreads prose.\nNothing else.".into()),
                model: Some(Some("model-b".into())),
                avatar: Some("seed-2".into()),
            },
            2_000.0,
            WAIT,
        )
        .unwrap();

        let row = &saved.row;
        assert_eq!(row.name, "Proofreader");
        assert_eq!(row.purpose, "Proofreads prose.\nNothing else.");
        assert_eq!(row.model.as_deref(), Some("model-b"));
        assert_eq!(row.avatar, "seed-2");
        assert_eq!(row.updated, 2_000.0);
        assert_eq!(row.slug, before.slug);
        assert_eq!(row.created, before.created);
        assert_eq!(row.home, before.home);
        assert_eq!(row.definition, before.definition);
        assert_eq!(row.session.as_deref(), Some("s-1"));
        assert_eq!(row.conversations, before.conversations);
        assert_eq!((row.archived, row.retired), (4, 700.0));
        assert_eq!(scratch.stored(), vec![row.to_value()]);
        let definition = scratch.definition("editor").unwrap();
        assert!(
            definition.contains("Proofreads prose.\nNothing else."),
            "{definition}"
        );
        assert!(
            definition.contains("description: 'Proofreads prose.'"),
            "{definition}"
        );
        assert!(definition.contains("model: 'model-b'"), "{definition}");
        assert!(!definition.contains("Reviews the change."), "{definition}");
    }

    #[test]
    fn an_edit_that_names_no_model_leaves_it_and_one_that_clears_it_removes_it() {
        let scratch = Scratch::new("edit-model");
        made_with_memory(&scratch);
        let edit = |changes: Changes| {
            edit_in(
                &scratch.home(),
                &scratch.data(),
                "editor",
                changes,
                NOW,
                WAIT,
            )
            .unwrap()
        };

        let left = edit(Changes {
            name: Some("Same Model".into()),
            ..Changes::default()
        });
        assert_eq!(left.row.model.as_deref(), Some("model-a"));
        assert!(
            scratch
                .definition("editor")
                .unwrap()
                .contains("model: 'model-a'")
        );

        let cleared = edit(Changes {
            model: Some(None),
            ..Changes::default()
        });
        assert_eq!(cleared.row.model, None);
        assert!(!scratch.definition("editor").unwrap().contains("model:"));
    }

    #[test]
    fn an_edit_whose_definition_is_refused_saves_no_row_change() {
        let scratch = Scratch::new("edit-refused");
        made_with_memory(&scratch);
        let rows = scratch.bots_text();
        let definition = scratch.definition("editor");

        let refused = edit_in(
            &scratch.home(),
            &scratch.data(),
            "editor",
            Changes {
                name: Some("Renamed".into()),
                purpose: Some("\u{85}".into()),
                ..Changes::default()
            },
            NOW,
            WAIT,
        )
        .unwrap_err();

        assert!(matches!(refused, Refused::Definition(_)), "{refused}");
        assert_eq!(scratch.bots_text(), rows);
        assert_eq!(scratch.definition("editor"), definition);
    }

    #[test]
    fn an_edit_of_a_bot_whose_definition_is_gone_saves_nothing() {
        let scratch = Scratch::new("edit-missing");
        made_with_memory(&scratch);
        std::fs::remove_file(scratch.home().join("agents/editor.md")).unwrap();
        let rows = scratch.bots_text();

        let refused = edit_in(
            &scratch.home(),
            &scratch.data(),
            "editor",
            Changes {
                name: Some("Renamed".into()),
                ..Changes::default()
            },
            NOW,
            WAIT,
        )
        .unwrap_err();

        assert!(matches!(refused, Refused::Definition(_)), "{refused}");
        assert_eq!(scratch.bots_text(), rows);
        assert_eq!(scratch.definition("editor"), None);
    }

    #[test]
    fn an_edit_of_a_bot_with_no_definition_writes_the_row_and_makes_none() {
        let scratch = Scratch::new("edit-no-definition");
        scratch.write_bots(&[scratch.entry("old")]);

        let saved = edit_in(
            &scratch.home(),
            &scratch.data(),
            "old",
            Changes {
                purpose: Some("Does another thing.".into()),
                ..Changes::default()
            },
            NOW,
            WAIT,
        )
        .unwrap();

        assert_eq!(saved.row.purpose, "Does another thing.");
        assert_eq!(saved.row.definition, None);
        assert_eq!(saved.row.updated, NOW);
        assert_eq!(scratch.stored(), vec![saved.row.to_value()]);
        assert!(!scratch.home().join("agents").exists());
    }

    #[test]
    fn an_edit_makes_a_home_that_was_removed_by_hand() {
        let scratch = Scratch::new("edit-home");
        let made = scratch.make("Homed");
        std::fs::remove_dir_all(&made.row.home).unwrap();

        let saved = edit_in(
            &scratch.home(),
            &scratch.data(),
            "homed",
            Changes::default(),
            NOW,
            WAIT,
        )
        .unwrap();

        assert!(saved.home_made);
        assert!(Path::new(&saved.row.home).is_dir());
    }

    #[test]
    fn an_edit_that_breaks_a_bound_or_names_no_bot_is_refused() {
        let scratch = Scratch::new("edit-refused-forms");
        scratch.make("Dev");
        let rows = scratch.bots_text();
        let changed = |changes: Changes| {
            edit_in(&scratch.home(), &scratch.data(), "dev", changes, NOW, WAIT).unwrap_err()
        };

        assert!(matches!(
            changed(Changes {
                name: Some(" ".into()),
                ..Changes::default()
            }),
            Refused::Name
        ));
        assert!(matches!(
            changed(Changes {
                purpose: Some("a\0".into()),
                ..Changes::default()
            }),
            Refused::Purpose
        ));
        assert!(matches!(
            changed(Changes {
                model: Some(Some("inherit".into())),
                ..Changes::default()
            }),
            Refused::Model
        ));
        assert!(matches!(
            changed(Changes {
                avatar: Some(" ".into()),
                ..Changes::default()
            }),
            Refused::Avatar
        ));
        let unknown = edit_in(
            &scratch.home(),
            &scratch.data(),
            "nobody",
            Changes::default(),
            NOW,
            WAIT,
        )
        .unwrap_err();
        assert!(matches!(unknown, Refused::NoSuchBot(slug) if slug == "nobody"));
        assert_eq!(scratch.bots_text(), rows);
    }

    #[test]
    fn setting_the_model_rewrites_the_definition_then_the_row_and_clears_with_none() {
        let scratch = Scratch::new("set-model");
        made_with_memory(&scratch);

        let set = set_model_in(
            &scratch.home(),
            &scratch.data(),
            "editor",
            Some("model-c"),
            3_000.0,
            WAIT,
        )
        .unwrap();

        assert_eq!(set.model.as_deref(), Some("model-c"));
        assert_eq!(set.updated, 3_000.0);
        assert_eq!(scratch.stored(), vec![set.to_value()]);
        assert!(
            scratch
                .definition("editor")
                .unwrap()
                .contains("model: 'model-c'")
        );

        let cleared = set_model_in(
            &scratch.home(),
            &scratch.data(),
            "editor",
            None,
            4_000.0,
            WAIT,
        )
        .unwrap();

        assert_eq!(cleared.model, None);
        assert_eq!(cleared.updated, 4_000.0);
        assert_eq!(cleared.session.as_deref(), Some("s-1"));
        assert!(!scratch.definition("editor").unwrap().contains("model:"));
    }

    #[test]
    fn setting_the_model_does_not_make_a_home_that_is_missing() {
        let scratch = Scratch::new("set-model-home");
        let made = scratch.make("Homed");
        std::fs::remove_dir_all(&made.row.home).unwrap();

        set_model_in(
            &scratch.home(),
            &scratch.data(),
            "homed",
            Some("m"),
            NOW,
            WAIT,
        )
        .unwrap();

        assert!(!Path::new(&made.row.home).exists());
    }

    #[test]
    fn an_archive_and_a_restore_do_not_make_a_home_that_is_missing() {
        let scratch = Scratch::new("archive-home");
        let made = scratch.make("Homed");
        std::fs::remove_dir_all(&made.row.home).unwrap();

        let archived = archive_in(&scratch.home(), &scratch.data(), "homed", NOW, WAIT).unwrap();
        let restored =
            restore_in(&scratch.home(), &scratch.data(), "homed", NOW + 1.0, WAIT).unwrap();

        assert!(!Path::new(&made.row.home).exists());
        assert_eq!(archived.updated, NOW);
        assert_eq!(archived.retired, NOW);
        assert_eq!(restored.updated, NOW + 1.0);
        assert_eq!(restored.retired, 0.0);
        assert_eq!(scratch.row("homed").updated, NOW + 1.0);
    }

    #[test]
    fn a_model_that_breaks_the_bound_or_a_missing_bot_is_refused_when_setting_it() {
        let scratch = Scratch::new("set-model-refused");
        made_with_memory(&scratch);
        let rows = scratch.bots_text();

        for model in ["inherit", "m\nn", " m", "", &"m".repeat(513)] {
            let refused = set_model_in(
                &scratch.home(),
                &scratch.data(),
                "editor",
                Some(model),
                NOW,
                WAIT,
            )
            .unwrap_err();
            assert!(matches!(refused, Refused::Model), "{model:?}");
        }
        let unknown = set_model_in(
            &scratch.home(),
            &scratch.data(),
            "nobody",
            Some("m"),
            NOW,
            WAIT,
        )
        .unwrap_err();
        assert!(matches!(unknown, Refused::NoSuchBot(_)));
        assert_eq!(scratch.bots_text(), rows);
    }

    #[cfg(unix)]
    #[test]
    fn ensuring_a_home_makes_a_missing_one_privately() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new("ensure");
        let made = scratch.make("Homed");
        std::fs::remove_dir_all(scratch.data()).unwrap();

        let ensured = ensure_home_in(&scratch.home(), &scratch.data(), "homed", NOW, WAIT).unwrap();

        assert!(ensured);
        let mode = |path: PathBuf| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(PathBuf::from(&made.row.home)), 0o700);
        assert_eq!(mode(scratch.data().join("bot-homes")), 0o700);
        assert_eq!(mode(scratch.data()), 0o700);
    }

    #[test]
    fn ensuring_a_home_changes_no_row_and_says_when_it_could_not_be_made() {
        let scratch = Scratch::new("ensure-blocked");
        scratch.make("Homed");
        let rows = scratch.bots_text();
        std::fs::remove_dir_all(scratch.data()).unwrap();
        std::fs::create_dir_all(scratch.data()).unwrap();
        std::fs::write(scratch.data().join("bot-homes"), "a file").unwrap();

        let ensured = ensure_home_in(&scratch.home(), &scratch.data(), "homed", NOW, WAIT).unwrap();

        assert!(!ensured);
        assert_eq!(scratch.bots_text(), rows);
    }

    #[test]
    fn ensuring_the_home_of_a_bot_nobody_made_or_from_a_file_that_does_not_parse_is_refused() {
        let scratch = Scratch::new("ensure-refused");
        scratch.make("Homed");

        let unknown =
            ensure_home_in(&scratch.home(), &scratch.data(), "nobody", NOW, WAIT).unwrap_err();
        assert!(matches!(unknown, Refused::NoSuchBot(slug) if slug == "nobody"));
        assert!(!scratch.data().join("bot-homes/nobody").exists());

        std::fs::write(scratch.home().join("bots.json"), "{ not json").unwrap();
        let unreadable =
            ensure_home_in(&scratch.home(), &scratch.data(), "homed", NOW, WAIT).unwrap_err();
        assert!(matches!(unreadable, Refused::Unreadable(_)), "{unreadable}");
    }

    #[test]
    fn a_home_the_row_names_is_the_one_made_whatever_the_data_folder() {
        let scratch = Scratch::new("ensure-row-home");
        let mut entry = scratch.entry("elsewhere");
        let chosen = scratch.0.join("chosen/home");
        entry["home"] = json!(chosen.to_str().unwrap());
        scratch.write_bots(&[entry]);

        let ensured =
            ensure_home_in(&scratch.home(), &scratch.data(), "elsewhere", NOW, WAIT).unwrap();

        assert!(ensured);
        assert!(chosen.is_dir());
        assert!(!scratch.data().join("bot-homes/elsewhere").exists());
    }

    #[cfg(unix)]
    struct RelativeDebris(&'static str);

    #[cfg(unix)]
    impl RelativeDebris {
        fn clear(home: &'static str) -> Self {
            let _ = std::fs::remove_dir_all(home);
            Self(home)
        }
    }

    #[cfg(unix)]
    impl Drop for RelativeDebris {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_home_in_the_other_systems_spelling_is_not_made_as_a_relative_folder() {
        let spellings = [r"C:\Users\x\bot-homes\dev", r"\\server\share\bot-homes\dev"];
        for spelling in spellings {
            let _debris = RelativeDebris::clear(spelling);
            let scratch = Scratch::new("other-spelling");
            let mut entry = scratch.entry("dev");
            entry["home"] = json!(spelling);
            scratch.write_bots(&[entry]);

            let ensured =
                ensure_home_in(&scratch.home(), &scratch.data(), "dev", NOW, WAIT).unwrap();
            let edited = edit_in(
                &scratch.home(),
                &scratch.data(),
                "dev",
                Changes::default(),
                NOW,
                WAIT,
            )
            .unwrap();

            assert!(!ensured, "{spelling}");
            assert!(!edited.home_made, "{spelling}");
            assert!(!Path::new(spelling).exists(), "{spelling}");
        }
    }

    fn without_stamps(entry: &Value) -> Value {
        let mut copy = entry.clone();
        let fields = copy.as_object_mut().expect("an object");
        fields.remove("retired");
        fields.remove("updated");
        copy
    }

    #[test]
    fn archiving_a_bot_that_is_archived_stamps_retired_again_and_leaves_the_definition() {
        let scratch = Scratch::new("archive");
        let before = made_with_memory(&scratch);
        let definition = scratch.home().join("agents/editor.md");
        let mut bytes = std::fs::read(&definition).unwrap();
        bytes.extend_from_slice(b"Hand edited.\n");
        std::fs::write(&definition, &bytes).unwrap();
        let other = scratch.entry("other");
        let kept = scratch.stored();
        scratch.write_bots(&[kept[0].clone(), other]);

        let first = archive_in(&scratch.home(), &scratch.data(), "editor", NOW, WAIT).unwrap();
        let later = NOW + 25.0;
        let second = archive_in(&scratch.home(), &scratch.data(), "editor", later, WAIT).unwrap();

        assert_eq!(first.retired, NOW);
        assert_eq!(first.updated, NOW);
        assert_eq!(second.retired, later);
        assert_ne!(second.retired, first.retired);
        assert_eq!(second.updated, later);
        assert_eq!(first.session, before.session);
        assert_eq!(second.session, before.session);
        assert_eq!(first.conversations, before.conversations);
        assert_eq!(second.conversations, before.conversations);
        assert_eq!(first.archived, before.archived);
        assert_eq!(second.archived, before.archived);
        assert_eq!(
            (
                second.slug.as_str(),
                second.created,
                second.definition.as_deref()
            ),
            ("editor", before.created, Some("editor"))
        );
        assert_eq!(second.name, before.name);
        assert_eq!(second.purpose, before.purpose);
        assert_eq!(second.home, before.home);
        assert_eq!(std::fs::read(&definition).unwrap(), bytes);
        let stored = scratch.stored();
        let editor = stored
            .iter()
            .find(|entry| entry["slug"] == "editor")
            .unwrap();
        assert_eq!(editor["retired"].as_f64(), Some(later));
        assert_eq!(without_stamps(editor), without_stamps(&before.to_value()));
        let untouched = stored
            .iter()
            .find(|entry| entry["slug"] == "other")
            .unwrap();
        assert_eq!(untouched, &scratch.entry("other"));

        let restored = restore_in(
            &scratch.home(),
            &scratch.data(),
            "editor",
            later + 5.0,
            WAIT,
        )
        .unwrap();
        assert_eq!(restored.retired, 0.0);
        assert_eq!(restored.session, before.session);
        assert_eq!(restored.conversations, before.conversations);
        assert_eq!(restored.archived, before.archived);
        assert_eq!(std::fs::read(&definition).unwrap(), bytes);
    }

    #[test]
    fn restoring_sets_retired_to_zero_and_each_write_stamps_updated() {
        let scratch = Scratch::new("restore");
        scratch.make("Keeper");
        archive_in(&scratch.home(), &scratch.data(), "keeper", NOW, WAIT).unwrap();

        let restored =
            restore_in(&scratch.home(), &scratch.data(), "keeper", NOW + 5.0, WAIT).unwrap();
        let again =
            restore_in(&scratch.home(), &scratch.data(), "keeper", NOW + 9.0, WAIT).unwrap();

        assert_eq!(restored.retired, 0.0);
        assert_eq!(restored.updated, NOW + 5.0);
        assert_eq!(again.retired, 0.0);
        assert_eq!(again.updated, NOW + 9.0);
        assert_ne!(again.updated, restored.updated);
        let row = scratch.stored().into_iter().next().unwrap();
        assert_eq!(row["retired"].as_f64(), Some(0.0));
        assert_eq!(row["updated"].as_f64(), Some(NOW + 9.0));
    }

    #[test]
    fn an_archive_or_a_restore_of_a_missing_bot_writes_nothing() {
        let scratch = Scratch::new("archive-missing");
        let made = scratch.make("Keeper");
        let definition = scratch.definition("keeper").unwrap();
        let rows = scratch.bots_text();

        let archived =
            archive_in(&scratch.home(), &scratch.data(), "ghost", NOW + 3.0, WAIT).unwrap_err();
        let restored =
            restore_in(&scratch.home(), &scratch.data(), "ghost", NOW + 4.0, WAIT).unwrap_err();

        assert!(matches!(archived, Refused::NoSuchBot(slug) if slug == "ghost"));
        assert!(matches!(restored, Refused::NoSuchBot(slug) if slug == "ghost"));
        assert_eq!(scratch.bots_text(), rows);
        assert_eq!(
            scratch.definition("keeper").as_deref(),
            Some(definition.as_str())
        );
        assert_eq!(scratch.row("keeper").retired, made.row.retired);
    }

    #[test]
    fn deleting_a_bot_removes_its_row_and_its_history_folder_and_nothing_else() {
        let scratch = Scratch::new("delete");
        let made = scratch.make("Keeper");
        scratch.make("Other");
        let definition = scratch.home().join("agents/keeper.md");
        let mut bytes = std::fs::read(&definition).unwrap();
        bytes.extend_from_slice(b"Hand edited.\n");
        std::fs::write(&definition, &bytes).unwrap();
        let session = scratch
            .home()
            .join("sessions/worked/33333333-3333-4333-8333-333333333333.json");
        std::fs::create_dir_all(session.parent().unwrap()).unwrap();
        std::fs::write(&session, "{\"id\":\"kept\"}").unwrap();
        let memory = scratch.0.join("work/.bravebot/memory/keeper.md");
        std::fs::create_dir_all(memory.parent().unwrap()).unwrap();
        std::fs::write(&memory, "notes").unwrap();
        let history = scratch.data().join("bots/keeper");
        std::fs::create_dir_all(history.join("nested")).unwrap();
        std::fs::write(history.join("nested/history.json"), "h").unwrap();
        let sibling = scratch.data().join("bots/other");
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("keep.txt"), "stay").unwrap();
        let home = PathBuf::from(&made.row.home);
        assert!(home.is_dir());
        let other_before = scratch
            .stored()
            .into_iter()
            .find(|entry| entry["slug"] == "other")
            .unwrap();

        delete_in(&scratch.home(), &scratch.data(), "keeper", NOW, WAIT).unwrap();

        let left: Vec<_> = scratch.stored();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0], other_before);
        assert!(!history.exists());
        assert_eq!(std::fs::read(sibling.join("keep.txt")).unwrap(), b"stay");
        assert_eq!(std::fs::read(&definition).unwrap(), bytes);
        assert_eq!(
            std::fs::read_to_string(&session).unwrap(),
            "{\"id\":\"kept\"}"
        );
        assert_eq!(std::fs::read_to_string(&memory).unwrap(), "notes");
        assert!(home.is_dir());
        assert!(scratch.definition("other").is_some());
    }

    #[test]
    fn a_delete_whose_history_folder_is_already_gone_removes_the_row() {
        let scratch = Scratch::new("delete-missing-folder");
        scratch.make("Keeper");
        assert!(!scratch.data().join("bots/keeper").exists());

        delete_in(&scratch.home(), &scratch.data(), "keeper", NOW, WAIT).unwrap();

        assert!(scratch.stored().is_empty());
    }

    #[test]
    fn a_history_folder_that_cannot_be_removed_leaves_the_row() {
        let scratch = Scratch::new("delete-history-file");
        scratch.make("Keeper");
        let rows = scratch.bots_text().unwrap();
        let history = scratch.data().join("bots/keeper");
        std::fs::create_dir_all(history.parent().unwrap()).unwrap();
        std::fs::write(&history, "not a directory").unwrap();

        let refused = delete_in(&scratch.home(), &scratch.data(), "keeper", NOW, WAIT).unwrap_err();

        assert!(matches!(refused, Refused::History(_)), "{refused:?}");
        let message = refused.to_string();
        assert!(message.contains("history folder"), "{message}");
        assert!(!message.contains("could not be saved"), "{message}");
        assert_eq!(scratch.bots_text().unwrap(), rows);
        assert_eq!(std::fs::read(&history).unwrap(), b"not a directory");
    }

    #[test]
    fn a_removal_that_fails_does_not_publish_the_row_deletion() {
        let scratch = Scratch::new("delete-unpublished");
        scratch.make("Keeper");
        let before = scratch.bots_text().unwrap();
        let home = scratch.home();
        let data = scratch.data();
        let during = before.clone();
        let folder = data.join("bots/keeper");

        let refused = modify_keeping(
            &home,
            &data,
            NOW,
            WAIT,
            |rows| {
                rows.retain(|row| row.slug != "keeper");
                Ok(())
            },
            |_| {
                assert_eq!(
                    scratch.bots_text().as_deref(),
                    Some(during.as_str()),
                    "the deletion is published before the folder removal fails"
                );
                Err(Refused::History(Problem::new(&folder, "not removed")))
            },
        )
        .unwrap_err();

        assert!(matches!(refused, Refused::History(_)), "{refused:?}");
        assert_eq!(scratch.bots_text().as_deref(), Some(before.as_str()));
    }

    #[test]
    fn a_slug_that_is_not_one_removes_nothing_and_a_missing_bot_removes_no_folder() {
        let scratch = Scratch::new("delete-slug");
        scratch.make("Keeper");
        let rows = scratch.bots_text().unwrap();
        let history = scratch.data().join("bots/keeper");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::write(history.join("history.json"), "h").unwrap();
        let ghost = scratch.data().join("bots/ghost");
        std::fs::create_dir_all(&ghost).unwrap();
        std::fs::write(ghost.join("history.json"), "g").unwrap();
        let escape = scratch.data().join("escape");
        std::fs::create_dir_all(&escape).unwrap();
        std::fs::write(escape.join("marker"), "stay").unwrap();
        let outside = scratch.0.join("escape");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("marker"), "stay").unwrap();
        std::fs::write(scratch.data().join("canary"), "stay").unwrap();
        let names = |folder: PathBuf| {
            let mut found = std::fs::read_dir(folder)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>();
            found.sort();
            found
        };
        let bots_before = names(scratch.data().join("bots"));

        for slug in [
            "../escape",
            "..",
            "../..",
            "/tmp/x",
            "keeper/../../escape",
            "",
            "Keeper",
            "a/b",
        ] {
            let refused = delete_in(&scratch.home(), &scratch.data(), slug, NOW, WAIT).unwrap_err();
            assert!(
                matches!(refused, Refused::NoSuchBot(ref got) if got == slug),
                "{slug:?} {refused:?}"
            );
        }
        let missing = delete_in(&scratch.home(), &scratch.data(), "ghost", NOW, WAIT).unwrap_err();
        assert!(matches!(missing, Refused::NoSuchBot(slug) if slug == "ghost"));

        assert_eq!(scratch.bots_text().unwrap(), rows);
        assert_eq!(std::fs::read(history.join("history.json")).unwrap(), b"h");
        assert_eq!(std::fs::read(ghost.join("history.json")).unwrap(), b"g");
        assert_eq!(std::fs::read(escape.join("marker")).unwrap(), b"stay");
        assert_eq!(std::fs::read(outside.join("marker")).unwrap(), b"stay");
        assert_eq!(
            std::fs::read(scratch.data().join("canary")).unwrap(),
            b"stay"
        );
        assert_eq!(names(scratch.data().join("bots")), bots_before);
    }

    fn plant(scratch: &Scratch, definition: &str) -> Vec<u8> {
        let path = scratch
            .home()
            .join("agents")
            .join(format!("{definition}.md"));
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.extend_from_slice(b"Hand edited.\n");
        std::fs::write(&path, &bytes).unwrap();
        bytes
    }

    fn same_identity(row: &Row, before: &Row) {
        assert_eq!(row.name, before.name);
        assert_eq!(row.purpose, before.purpose);
        assert_eq!(row.model, before.model);
        assert_eq!(row.avatar, before.avatar);
        assert_eq!(row.home, before.home);
        assert_eq!(row.definition, before.definition);
        assert_eq!(row.retired, before.retired);
        assert_eq!(row.created, before.created);
    }

    fn absolute(scratch: &Scratch, name: &str) -> String {
        scratch.data().join(name).to_str().unwrap().to_string()
    }

    fn wrote(result: Result<Noted, Refused>) -> Row {
        let noted = result.expect("the note was accepted");
        assert!(noted.wrote);
        noted.row.expect("the note wrote a row")
    }

    fn held(result: Result<Noted, Refused>) -> Row {
        let noted = result.expect("the note was accepted");
        assert!(!noted.wrote);
        noted.row.expect("the row was still there")
    }

    fn absent(result: Result<Noted, Refused>) {
        let noted = result.expect("a missing bot is not a refusal");
        assert!(!noted.wrote);
        assert!(noted.row.is_none());
    }

    #[test]
    fn a_recorded_conversation_sets_the_session_and_is_added_once() {
        let scratch = Scratch::new("record-conversation");
        let made = scratch.make("Keeper");
        let before = scratch.row("keeper");
        let bytes = plant(&scratch, "keeper");
        let definition = scratch.home().join("agents/keeper.md");
        let dir_a = absolute(&scratch, "proj-a");
        let dir_b = absolute(&scratch, "proj-b");

        let first = wrote(record_conversation_in(
            &scratch.home(),
            &scratch.data(),
            "keeper",
            "s-1",
            &dir_a,
            NOW + 1.0,
            WAIT,
        ));

        assert_eq!(first.session.as_deref(), Some("s-1"));
        assert_eq!(
            first.conversations,
            vec![Conversation {
                id: "s-1".into(),
                directory: dir_a.clone(),
            }]
        );
        assert_eq!(first.updated, NOW + 1.0);
        same_identity(&first, &before);
        assert_eq!(first.archived, before.archived);
        assert_eq!(std::fs::read(&definition).unwrap(), bytes);

        let second = wrote(record_conversation_in(
            &scratch.home(),
            &scratch.data(),
            "keeper",
            "s-1",
            &dir_a,
            NOW + 2.0,
            WAIT,
        ));

        assert_eq!(
            second.conversations.len(),
            1,
            "the conversation was added twice: {:?}",
            second.conversations
        );
        assert_eq!(second.conversations, first.conversations);
        assert_eq!(second.session.as_deref(), Some("s-1"));
        assert_eq!(second.updated, NOW + 2.0);
        assert_ne!(second.updated, first.updated);
        same_identity(&second, &before);
        assert_eq!(std::fs::read(&definition).unwrap(), bytes);

        let moved = wrote(record_conversation_in(
            &scratch.home(),
            &scratch.data(),
            "keeper",
            "s-1",
            &dir_b,
            NOW + 3.0,
            WAIT,
        ));

        assert_eq!(moved.session.as_deref(), Some("s-1"));
        assert_eq!(
            moved.conversations,
            vec![
                Conversation {
                    id: "s-1".into(),
                    directory: dir_a,
                },
                Conversation {
                    id: "s-1".into(),
                    directory: dir_b,
                },
            ]
        );
        assert_eq!(moved.updated, NOW + 3.0);
        same_identity(&moved, &before);
        assert_eq!(moved.retired, made.row.retired);
        assert_eq!(std::fs::read(&definition).unwrap(), bytes);

        let text = scratch.bots_text();
        absent(record_conversation_in(
            &scratch.home(),
            &scratch.data(),
            "ghost",
            "s-1",
            &absolute(&scratch, "proj-a"),
            NOW + 4.0,
            WAIT,
        ));
        assert_eq!(scratch.bots_text(), text);
        assert_eq!(std::fs::read(&definition).unwrap(), bytes);
    }

    #[test]
    fn a_conversation_the_parser_would_not_keep_is_refused_and_writes_nothing() {
        let scratch = Scratch::new("record-refused");
        let before = scratch.make("Keeper").row;
        let bytes = plant(&scratch, "keeper");
        let definition = scratch.home().join("agents/keeper.md");
        let text = scratch.bots_text();
        let directory = absolute(&scratch, "proj-a");
        for (id, directory) in [
            ("a/b", directory.as_str()),
            ("", directory.as_str()),
            ("s\0", directory.as_str()),
            ("s-1", "relative"),
            ("s-1", ""),
            ("s-1", "rel\0ative"),
        ] {
            let refused = record_conversation_in(
                &scratch.home(),
                &scratch.data(),
                "keeper",
                id,
                directory,
                NOW + 1.0,
                WAIT,
            )
            .expect_err(id);
            assert!(matches!(refused, Refused::Conversation), "{id} {directory}");
            assert_eq!(scratch.bots_text(), text, "{id}");
            assert_eq!(scratch.row("keeper").session, before.session, "{id}");
            assert_eq!(std::fs::read(&definition).unwrap(), bytes);
        }
        absent(record_conversation_in(
            &scratch.home(),
            &scratch.data(),
            "ghost",
            "a/b",
            "relative",
            NOW + 1.0,
            WAIT,
        ));
        assert_eq!(scratch.bots_text(), text);
        let kept = wrote(record_conversation_in(
            &scratch.home(),
            &scratch.data(),
            "keeper",
            "s-1",
            &directory,
            NOW + 2.0,
            WAIT,
        ));
        assert_eq!(kept.session.as_deref(), Some("s-1"));
        assert_eq!(kept.slug, before.slug);
    }

    #[test]
    fn an_archive_length_only_rises() {
        let scratch = Scratch::new("record-archived");
        let before = made_with_memory(&scratch);
        let bytes = plant(&scratch, "editor");
        let definition = scratch.home().join("agents/editor.md");
        let text = scratch.bots_text();
        assert_eq!(before.archived, 4);

        for value in [4.0, 3.0, 0.0, -1.0, 4.5, f64::NAN, MAX_SAFE_INTEGER + 2.0] {
            let answered = held(record_archived_in(
                &scratch.home(),
                &scratch.data(),
                "editor",
                value,
                NOW + 20.0,
                WAIT,
            ));
            assert_eq!(answered.archived, 4, "{value}");
            assert_eq!(answered.updated, before.updated, "{value}");
            assert_eq!(scratch.bots_text(), text, "{value}");
        }

        absent(record_archived_in(
            &scratch.home(),
            &scratch.data(),
            "ghost",
            9.0,
            NOW + 21.0,
            WAIT,
        ));
        assert_eq!(scratch.bots_text(), text);

        let risen = wrote(record_archived_in(
            &scratch.home(),
            &scratch.data(),
            "editor",
            5.0,
            NOW + 22.0,
            WAIT,
        ));

        assert_eq!(risen.archived, 5);
        assert_eq!(risen.updated, NOW + 22.0);
        assert_eq!(risen.session, before.session);
        assert_eq!(risen.conversations, before.conversations);
        same_identity(&risen, &before);
        assert_eq!(std::fs::read(&definition).unwrap(), bytes);
        assert_eq!(scratch.row("editor").archived, 5);

        let top = wrote(record_archived_in(
            &scratch.home(),
            &scratch.data(),
            "editor",
            MAX_SAFE_INTEGER,
            NOW + 23.0,
            WAIT,
        ));
        assert_eq!(top.archived, MAX_SAFE_INTEGER as u64);
        assert_eq!(top.updated, NOW + 23.0);
        same_identity(&top, &before);
        assert_eq!(std::fs::read(&definition).unwrap(), bytes);
    }

    #[test]
    fn releasing_a_session_clears_it_and_keeps_the_conversation() {
        let scratch = Scratch::new("release-session");
        let before = made_with_memory(&scratch);
        let bytes = plant(&scratch, "editor");
        let definition = scratch.home().join("agents/editor.md");

        let released = wrote(release_session_in(
            &scratch.home(),
            &scratch.data(),
            "editor",
            NOW + 1.0,
            WAIT,
        ));

        assert_eq!(released.session, None);
        assert_eq!(released.archived, 0);
        assert_eq!(released.conversations, before.conversations);
        assert_eq!(released.updated, NOW + 1.0);
        same_identity(&released, &before);
        assert_eq!(std::fs::read(&definition).unwrap(), bytes);
        assert_eq!(scratch.row("editor").conversations, before.conversations);

        let text = scratch.bots_text();
        let again = held(release_session_in(
            &scratch.home(),
            &scratch.data(),
            "editor",
            NOW + 9.0,
            WAIT,
        ));
        assert_eq!(again.session, None);
        assert_eq!(again.archived, 0);
        assert_eq!(again.updated, NOW + 1.0);
        assert_eq!(again.conversations, before.conversations);
        assert_eq!(scratch.bots_text(), text);
        assert_eq!(std::fs::read(&definition).unwrap(), bytes);

        let mut entries = scratch.stored();
        entries[0]["archived"] = json!(9);
        scratch.write_bots(&entries);
        let parked = scratch.bots_text();
        let left = held(release_session_in(
            &scratch.home(),
            &scratch.data(),
            "editor",
            NOW + 11.0,
            WAIT,
        ));
        assert_eq!(left.session, None);
        assert_eq!(left.archived, 9);
        assert_eq!(left.updated, released.updated);
        assert_eq!(scratch.bots_text(), parked);

        absent(release_session_in(
            &scratch.home(),
            &scratch.data(),
            "ghost",
            NOW + 12.0,
            WAIT,
        ));
        assert_eq!(scratch.bots_text(), parked);
        assert_eq!(std::fs::read(&definition).unwrap(), bytes);
    }

    #[test]
    fn forgetting_a_deleted_conversation_removes_it_from_every_row_that_lists_it() {
        let scratch = Scratch::new("forget-conversation");
        for name in ["Keeper", "Listed", "Pointed", "Untouched"] {
            scratch.make(name);
        }
        let dir = absolute(&scratch, "proj-a");
        let other = absolute(&scratch, "proj-b");
        let mut entries = scratch.stored();
        let set = |entries: &mut [Value],
                   slug: &str,
                   session: &str,
                   archived: u64,
                   pairs: &[(&str, &str)]| {
            let entry = entries
                .iter_mut()
                .find(|entry| entry["slug"] == slug)
                .unwrap();
            entry["session"] = json!(session);
            entry["archived"] = json!(archived);
            entry["updated"] = json!(6);
            entry["conversations"] = json!(
                pairs
                    .iter()
                    .map(|(id, directory)| json!({"id": id, "directory": directory}))
                    .collect::<Vec<_>>()
            );
        };
        set(
            &mut entries,
            "keeper",
            "s-1",
            4,
            &[("s-1", &dir), ("s-2", &other)],
        );
        set(&mut entries, "listed", "s-9", 2, &[("s-1", &dir)]);
        set(&mut entries, "pointed", "s-1", 7, &[("s-1", &other)]);
        set(&mut entries, "untouched", "s-3", 1, &[("s-3", &other)]);
        scratch.write_bots(&entries);
        let before = |slug: &str| scratch.row(slug);
        let keeper = before("keeper");
        let listed = before("listed");
        let pointed = before("pointed");
        let untouched = before("untouched");
        let bytes: Vec<_> = ["keeper", "listed", "pointed", "untouched"]
            .into_iter()
            .map(|name| (name, plant(&scratch, name)))
            .collect();

        let changed = forget_conversation_in(
            &scratch.home(),
            &scratch.data(),
            "s-1",
            &dir,
            NOW + 50.0,
            WAIT,
        )
        .unwrap();

        assert_eq!(changed, 3);
        let keeper_after = scratch.row("keeper");
        let listed_after = scratch.row("listed");
        let pointed_after = scratch.row("pointed");
        let untouched_after = scratch.row("untouched");
        assert_eq!(keeper_after.session, None);
        assert_eq!(keeper_after.archived, 0);
        assert_eq!(
            keeper_after.conversations,
            vec![Conversation {
                id: "s-2".into(),
                directory: other.clone(),
            }]
        );
        assert_eq!(keeper_after.updated, NOW + 50.0);
        assert_eq!(listed_after.session.as_deref(), Some("s-9"));
        assert_eq!(listed_after.archived, 2);
        assert!(
            listed_after
                .conversations
                .iter()
                .all(|each| each.id != "s-1"),
            "the deleted pair stayed: {:?}",
            listed_after.conversations
        );
        let listed_stored = scratch
            .stored()
            .into_iter()
            .find(|entry| entry["slug"] == "listed")
            .unwrap();
        assert!(
            listed_stored["conversations"]
                .as_array()
                .unwrap()
                .iter()
                .all(|each| each["id"] != "s-1"),
            "the deleted pair stayed on disk: {}",
            listed_stored["conversations"]
        );
        assert_eq!(listed_stored["session"], json!("s-9"));
        assert_eq!(listed_stored["archived"], json!(2));
        assert_eq!(listed_after.updated, NOW + 50.0);
        assert_eq!(pointed_after.session, None);
        assert_eq!(pointed_after.archived, 0);
        assert_eq!(
            pointed_after.conversations,
            vec![Conversation {
                id: "s-1".into(),
                directory: other,
            }]
        );
        assert_eq!(pointed_after.updated, NOW + 50.0);
        assert_eq!(untouched_after.updated, untouched.updated);
        assert_eq!(untouched_after.session, untouched.session);
        assert_eq!(untouched_after.archived, untouched.archived);
        assert_eq!(untouched_after.conversations, untouched.conversations);
        for (row, prior) in [
            (&keeper_after, &keeper),
            (&listed_after, &listed),
            (&pointed_after, &pointed),
            (&untouched_after, &untouched),
        ] {
            same_identity(row, prior);
        }
        for (name, bytes) in bytes {
            assert_eq!(
                std::fs::read(scratch.home().join("agents").join(format!("{name}.md"))).unwrap(),
                bytes,
                "{name}"
            );
        }

        let text = scratch.bots_text();
        let none = forget_conversation_in(
            &scratch.home(),
            &scratch.data(),
            "s-missing",
            &dir,
            NOW + 80.0,
            WAIT,
        )
        .unwrap();
        assert_eq!(none, 0);
        assert_eq!(scratch.bots_text(), text);
        assert_eq!(scratch.row("untouched").updated, untouched.updated);
        assert_eq!(scratch.row("keeper").updated, NOW + 50.0);
    }
}
