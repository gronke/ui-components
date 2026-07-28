//! The qr feature's registry gate, in a consuming binary: the
//! `data-tui="qr"` registration must survive the linker (the `link()`
//! anchor), mount through the inventory registry during a host commit and
//! paint its half-block render — a silently dropped registration would
//! degrade the element to a generic container and paint nothing.

use ratatui::backend::TestBackend;
use ratatui::style::Color;
use ratatui::Terminal;
use uic_tui::dom::HostState;

#[test]
fn the_qr_registration_mounts_and_paints() {
    uic_tui::qr::link();
    let mut state = HostState::new();
    let root = state.create_root("qr-host", &[]);
    // The scripted-host commit is the real mount path: it consults the
    // widget registry and syncs the value attribute into the adapter.
    state.commit(root, r#"<div data-tui="qr" value="somePairingCode"></div>"#);

    let mut terminal = Terminal::new(TestBackend::new(60, 30)).expect("test terminal");
    terminal
        .draw(|frame| uic_tui::dom::paint_document(frame, frame.area(), &mut state.doc, None))
        .expect("draw");

    // The widget paints the code black on white; a QR-sized patch of cells
    // carries half-block glyphs in exactly that card style.
    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    let mut carded = 0;
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buffer[(x, y)];
            if cell.fg == Color::Rgb(0, 0, 0)
                && cell.bg == Color::Rgb(255, 255, 255)
                && cell.symbol().chars().all(|c| "█▀▄ ".contains(c))
            {
                carded += 1;
            }
        }
    }
    assert!(
        carded >= 21 * 10,
        "a QR paints as a black-on-white half-block card ({carded} cells)"
    );
}
