//! The dialog box under a test terminal: it paints centered over whatever
//! lies beneath, and its keystroke handler answers the way a browser
//! dialog does: Enter for the focused button, Escape cancels, y/n decide
//! a confirm, printables land in a prompt's line.

use uic_tui::dialog::{paint_dialog, Dialog, DialogChoice, DialogOutcome};
use uic_tui::ratatui::backend::TestBackend;
use uic_tui::ratatui::widgets::Paragraph;
use uic_tui::ratatui::Terminal;
use uic_tui::KeyStroke;

fn screen(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    let mut out = String::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn the_box_paints_centered_over_the_page() {
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    let dialog = Dialog::confirm("accept the new pairing?");
    terminal
        .draw(|frame| {
            // A page full of noise beneath: the dialog must wipe its rect.
            for y in 0..12 {
                let row = uic_tui::ratatui::layout::Rect::new(0, y, 60, 1);
                frame.render_widget(Paragraph::new("#".repeat(60)), row);
            }
            paint_dialog(frame, frame.area(), &dialog);
        })
        .unwrap();
    let screen = screen(&terminal);
    assert!(
        screen.contains("accept the new pairing?"),
        "the message shows: {screen}"
    );
    assert!(screen.contains("[ ok ]"), "the ok button shows: {screen}");
    assert!(
        screen.contains("[ cancel ]"),
        "the cancel button shows: {screen}"
    );
    assert!(screen.contains("confirm"), "the title shows: {screen}");
    let boxed = screen
        .lines()
        .find(|line| line.contains("accept the new pairing?"))
        .unwrap();
    assert!(
        boxed.starts_with('#') && boxed.contains("│"),
        "the page shows around the box, the box wipes within: {boxed}"
    );
}

#[test]
fn enter_answers_with_the_focused_button() {
    let mut dialog = Dialog::confirm("sure?");
    assert_eq!(dialog.key(&KeyStroke::new("Enter")), DialogOutcome::Ok);

    let mut dialog = Dialog::confirm("sure?");
    assert_eq!(dialog.key(&KeyStroke::new("Tab")), DialogOutcome::Open);
    assert_eq!(dialog.focus, DialogChoice::Cancel);
    assert_eq!(dialog.key(&KeyStroke::new("Enter")), DialogOutcome::Cancel);
    assert_eq!(
        dialog.key(&KeyStroke::new("ArrowLeft")),
        DialogOutcome::Open
    );
    assert_eq!(dialog.focus, DialogChoice::Ok);
}

#[test]
fn escape_cancels_and_shortcuts_decide_a_confirm() {
    let mut dialog = Dialog::confirm("sure?");
    assert_eq!(dialog.key(&KeyStroke::new("Escape")), DialogOutcome::Cancel);
    assert_eq!(dialog.key(&KeyStroke::new("y")), DialogOutcome::Ok);
    assert_eq!(dialog.key(&KeyStroke::new("n")), DialogOutcome::Cancel);

    // An alert has one button and no focus to move.
    let mut alert = Dialog::alert("done");
    assert_eq!(alert.key(&KeyStroke::new("Tab")), DialogOutcome::Open);
    assert_eq!(alert.focus, DialogChoice::Ok);
    assert_eq!(alert.key(&KeyStroke::new("y")), DialogOutcome::Open);
    assert_eq!(alert.key(&KeyStroke::new("Enter")), DialogOutcome::Ok);
}

#[test]
fn a_prompt_collects_text_and_y_is_just_a_letter() {
    let mut dialog = Dialog::prompt("name?", "wor");
    assert_eq!(dialog.key(&KeyStroke::new("l")), DialogOutcome::Open);
    assert_eq!(dialog.key(&KeyStroke::new("d")), DialogOutcome::Open);
    assert_eq!(dialog.input, "world");
    assert_eq!(
        dialog.key(&KeyStroke::new("Backspace")),
        DialogOutcome::Open
    );
    assert_eq!(dialog.input, "worl");
    assert_eq!(dialog.key(&KeyStroke::new("y")), DialogOutcome::Open);
    assert_eq!(dialog.input, "worly");
    assert_eq!(dialog.key(&KeyStroke::new("Enter")), DialogOutcome::Ok);

    // Modifier chords stay with the host.
    let mut ctrl_c = KeyStroke::new("c");
    ctrl_c.ctrl = true;
    assert_eq!(dialog.key(&ctrl_c), DialogOutcome::Open);
    assert_eq!(dialog.input, "worly");
}

#[test]
fn the_prompt_paints_its_line_and_custom_labels_render() {
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    let mut dialog = Dialog::confirm("a different pairing link arrived — accept it?");
    dialog.ok_label = "accept".into();
    dialog.cancel_label = "keep waiting".into();
    terminal
        .draw(|frame| paint_dialog(frame, frame.area(), &dialog))
        .unwrap();
    let painted = screen(&terminal);
    assert!(painted.contains("[ accept ]"), "{painted}");
    assert!(painted.contains("[ keep waiting ]"), "{painted}");

    let prompt = Dialog::prompt("who?", "typed so far");
    terminal
        .draw(|frame| paint_dialog(frame, frame.area(), &prompt))
        .unwrap();
    let painted = screen(&terminal);
    assert!(
        painted.contains("typed so far▏"),
        "the input line paints with its caret: {painted}"
    );
}
