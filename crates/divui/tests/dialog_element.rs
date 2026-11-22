//! `Element::create("dialog")` makes the engine's dialog (RFC 0015 PR 3a):
//! hidden until open, then positioned by the user agent rules.

use div::typed_style::{Display, Position};
use div::{Document, Element};

#[test]
fn a_created_dialog_is_hidden_until_open() {
    let document = Document::new(480, 400).unwrap();
    for tag in ["dialog", "DIALOG"] {
        let dialog = Element::create(&document, tag).unwrap();
        document.body().append_child(&dialog).unwrap();
        assert_eq!(dialog.computed_style().unwrap().display, Display::None);
        dialog.set_attribute("open", "").unwrap();
        let style = dialog.computed_style().unwrap();
        assert_eq!(style.display, Display::Block, "{tag}");
        assert_eq!(style.position, Position::Absolute, "{tag}");
    }
}
