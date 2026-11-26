use super::*;

/// A read hands the token over and a slot fill does not. What a slot holds is the file's text,
/// for a processor to work on or a write to put back, so a note about the file appended there
/// would put a line into every file that went through one.
#[test]
fn a_slot_is_filled_with_the_file_and_a_read_also_carries_its_token() {
    let page = Page {
        lines: vec!["alpha".to_string()],
        ends_with_newline: true,
        first_line: 1,
        total_lines: 1,
        long_lines: 0,
        change_token: "0123456789abcdef".to_string(),
        decoding: bravebot_filetype::Decoding::Utf8,
    };

    assert_eq!(render_page(&page, ChangeToken::Withheld, true), "alpha");
    assert_eq!(
        render_page(&page, ChangeToken::Shown, true),
        "alpha\n\n(change token 0123456789abcdef)"
    );
}

/// A page that is a decoding says so, in words that name the encoding and say that an edit of the
/// file is refused, so the planner does not take a replacement character for something in the
/// file or an edit of it for something that can land. A page of the file's own UTF-8 says
/// nothing, since a note on every read would be a line in every file.
#[test]
fn a_decoded_page_says_how_it_was_decoded() {
    use bravebot_filetype::Decoding;

    let page_of = |decoding| Page {
        lines: vec!["alpha".to_string()],
        ends_with_newline: true,
        first_line: 1,
        total_lines: 1,
        long_lines: 0,
        change_token: "0123456789abcdef".to_string(),
        decoding,
    };
    let consequence = "so edit_file cannot change this file";

    assert_eq!(
        render_page(&page_of(Decoding::Utf8), ChangeToken::Withheld, true),
        "alpha"
    );
    for (decoding, how) in [
        (
            Decoding::Utf8Replaced,
            "bytes that are not valid UTF-8 are shown as U+FFFD",
        ),
        (Decoding::Utf16Le, "decoded from UTF-16 little-endian"),
        (Decoding::Utf16Be, "decoded from UTF-16 big-endian"),
        (Decoding::Utf32Le, "decoded from UTF-32 little-endian"),
        (Decoding::Utf32Be, "decoded from UTF-32 big-endian"),
    ] {
        assert_eq!(
            render_page(&page_of(decoding), ChangeToken::Withheld, true),
            format!("alpha\n\n({how}, {consequence})"),
            "{decoding:?}"
        );
        assert_eq!(
            render_page(&page_of(decoding), ChangeToken::Withheld, false),
            format!("alpha\n\n({how})"),
            "{decoding:?} without edit_file"
        );
    }
}

/// TOOL-6: `request_path` names only the file tools on `Beside` after a grant opens the
/// directory, and `Beside::every()` keeps the full five-tool list.
#[test]
fn request_path_approval_names_only_offered_file_tools() {
    let full = request_path_file_tools_note(Beside::every(), false);
    assert!(
        full.contains("(read_file, write_file, edit_file, list_files, search)"),
        "{full}"
    );
    let partial = request_path_file_tools_note(
        Beside::every()
            .without(Named::EditFile)
            .without(Named::Search),
        true,
    );
    assert!(
        partial.contains("(read_file, write_file, list_files)")
            && !partial.contains("edit_file")
            && !partial.contains("search")
            && partial.contains("no delegate is given a checkout"),
        "{partial}"
    );
    let none = request_path_file_tools_note(
        Beside::every()
            .without(Named::ReadFile)
            .without(Named::WriteFile)
            .without(Named::EditFile)
            .without(Named::ListFiles)
            .without(Named::Search),
        false,
    );
    assert!(
        !none.contains("read_file")
            && !none.contains("write_file")
            && !none.contains("edit_file")
            && !none.contains("list_files")
            && !none.contains("search")
            && none.contains("/add-dir"),
        "{none}"
    );
}

/// A slot is filled with the text a file in another encoding decodes to, marked as a decoding,
/// and the text carries no note about the decoding: the note is for the planner reading a page,
/// and a slot's text is the document a processor works on. A UTF-8 file is not marked.
#[test]
fn a_slot_filled_from_a_file_in_another_encoding_is_marked_and_carries_no_note() {
    let dir = crate::testutil::scratch_dir(&format!(
        "bravebot-slot-decoded-fill-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch");
    let wide: Vec<u8> = [0xFFu8, 0xFE]
        .into_iter()
        .chain("alpha\nbeta\n".encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    std::fs::write(dir.join("wide.txt"), wide).unwrap();
    std::fs::write(dir.join("latin1.txt"), b"caf\xe9\n").unwrap();
    std::fs::write(dir.join("plain.txt"), "plain\n").unwrap();
    let workspace = Workspace::new(&dir).expect("workspace");

    let wide = read_into_slot(&workspace, "wide.txt").expect("a UTF-16 file reads");
    assert_eq!((wide.text.as_str(), wide.decoded), ("alpha\nbeta\n", true));
    let latin1 = read_into_slot(&workspace, "latin1.txt").expect("a Latin-1 file reads");
    assert_eq!(
        (latin1.text.as_str(), latin1.decoded),
        ("caf\u{fffd}\n", true)
    );
    let plain = read_into_slot(&workspace, "plain.txt").expect("a UTF-8 file reads");
    assert_eq!((plain.text.as_str(), plain.decoded), ("plain\n", false));
}
