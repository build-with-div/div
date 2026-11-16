//! A `TextNode` is a handle: dropping it leaves the node in the document, as
//! dropping an `Element` does. Before this, `Element::create_text_child`
//! returned a handle that removed its node on drop, so
//! `element.create_text_child("…")?;` never painted.

use div::prelude::*;

fn document() -> Document {
    Document::new(96, 32).unwrap()
}

#[test]
fn a_text_child_stays_when_its_handle_is_dropped() {
    let document = document();
    let body = document.body();
    body.create_text_child("dropped").unwrap();
    {
        let kept = body.create_text_child("kept").unwrap();
        assert_eq!(kept.data().unwrap(), "kept");
    }
    assert_eq!(body.text_content().unwrap(), "droppedkept");

    // `remove` still removes it.
    let removed = body.create_text_child("removed").unwrap();
    removed.remove().unwrap();
    assert_eq!(body.text_content().unwrap(), "droppedkept");
}

#[test]
fn a_text_child_paints_as_set_text_does() {
    let blank = document().render_to_png_buffer().unwrap();
    let with_set_text = {
        let document = document();
        document.body().set_text("Div").unwrap();
        document.render_to_png_buffer().unwrap()
    };
    let with_text_child = {
        let document = document();
        document.body().create_text_child("Div").unwrap();
        document.render_to_png_buffer().unwrap()
    };
    assert_ne!(with_set_text, blank, "the text paints");
    assert_eq!(with_text_child, with_set_text);
}

#[test]
fn view_dynamic_text_is_removed_when_its_scope_is_disposed() {
    let document = document();
    let parent = Element::create(&document, "div").unwrap();
    document.body().append_child(&parent).unwrap();
    let count = create_signal(1);
    let scope = create_scope(|| {
        with_document(&document, || {
            mount_view(
                &parent,
                view! { <span id="label">{count.get()}</span> }.into_view(),
            );
        });
    });
    let label = document.element_by_id("label").unwrap().unwrap();
    assert_eq!(label.text_content().unwrap(), "1");
    count.set(2);
    assert_eq!(label.text_content().unwrap(), "2");

    dispose_scope(scope);
    assert_eq!(
        label.text_content().unwrap(),
        "",
        "the scope's cleanup removed the dynamic text"
    );
    count.set(3);
    assert_eq!(label.text_content().unwrap(), "");
}
