//! `<dialog>` and the top layer (RFC 0015, platform-plan item H4).
//!
//! The behavior is Blink 147's (`html_dialog_element.cc`, BSD), with its
//! legacy focusing steps (`DialogNewFocusBehavior` is experimental in 147)
//! and without closing on `open` removal (`DialogCloseWhenOpenRemoved` is
//! experimental too):
//!
//! - `show_modal` puts the dialog at the top of the document's top layer and
//!   makes it modal; `close` and removal from the document take it out.
//!   Removing only the `open` attribute leaves it modal and in the top
//!   layer.
//! - While a modal dialog is in the top layer, everything outside the
//!   topmost one is inert: it cannot be focused, is not in the Tab order or
//!   the accessibility tree, and is not hit by pointer input.
//!
//! The events (`beforetoggle`, `toggle`, `cancel`, `close`), Escape and the
//! public `divui` API are the caller's (RFC 0015 PR 5). A dialog is an
//! element of kind `Div` whose local name is `dialog` (RFC 0004's local
//! names).

use std::collections::HashMap;

use div_style::selector::{
    AttributeMatcher, AttributeSelector, Compound, PseudoClass, Selector, SimpleSelector,
};
use div_style::stylesheet::{CascadeOrigin, StyleRule, StyleSheet};
use div_style::{parse_style_attribute, Declaration, Length, LengthValue, Style, StyleProperty};

use crate::{Engine, EngineError, FocusOrigin, InvalidationClass, NodeHandle};

/// Blink's `InvalidStateError` messages.
const SHOW_ON_MODAL: &str =
    "The dialog is already open as a modal dialog, and therefore cannot be opened as a non-modal dialog.";
const SHOW_MODAL_ON_NON_MODAL: &str =
    "The dialog is already open as a non-modal dialog, and therefore cannot be opened as a modal dialog.";
const SHOW_MODAL_DISCONNECTED: &str = "The element is not in a Document.";

/// The `dialog` element rules of the HTML Standard's rendering section, as
/// far as Div can state them: selectors and declaration blocks, in order.
/// They are adapted from the specification text, not taken from Blink's
/// `html.css`, and they are not the specification's rules verbatim.
///
/// Source: WHATWG HTML Living Standard, section 15.3.3 "Flow content"
/// (<https://html.spec.whatwg.org/multipage/rendering.html#flow-content-3>),
/// © WHATWG, CC BY 4.0, as last updated 20 November 2026; the copy the
/// fidelity lane fetched for RFC 0004 has sha256
/// `7c8fbef654ba0090903a110e7bf3dacff1bf763184a808d7a35be7b05e12b764`.
/// `::backdrop` arrives with RFC 0015 PR 4.
///
/// These restate the specification's value exactly:
///
/// - Its `display: block` for `dialog` is a `Div`'s own display.
/// - `fit-content` is not a literal Div's style parser reads; the sheet sets
///   [`Length::fit_content`] for `width` and `height` directly.
/// - `border: solid` is written as its longhands; `medium` is 3px, which
///   Div's border widths take as a number of pixels.
///
/// These are gaps, where a dialog differs from the specification until
/// their owner closes them:
///
/// - **Logical insets** (engine, platform-plan item S7). Div has no
///   `inset-inline` or `inset-block`, so the sheet uses `left`, `right`,
///   `top` and `bottom`. A non-modal dialog in a vertical writing mode gets
///   the wrong axis.
/// - **System colors** (engine, with the `color-scheme` property of item S7
///   and RFC 0014's `Document::set_color_scheme`). Div has no `Canvas` or
///   `CanvasText`, so the sheet uses their light-scheme values, white and
///   black. A dialog in a dark color scheme keeps them.
/// - **`dialog:modal`'s `max-width` and `max-height`**,
///   `calc(100% - 6px - 2em)` (engine 0.2 PR 2: `calc()` in `width`,
///   `height`, `min-*` and `max-*`, with `em` and `%`). Div's lengths cannot hold a percentage less a
///   font-relative length, so a modal dialog larger than the viewport is
///   not clamped.
pub(crate) const DIALOG_USER_AGENT_RULES: &[(&str, &str)] = &[
    (
        "dialog",
        concat!(
            "position: absolute; left: 0; right: 0; margin: auto; ",
            "border-top-style: solid; border-top-width: 3; border-top-color: currentcolor; ",
            "border-right-style: solid; border-right-width: 3; border-right-color: currentcolor; ",
            "border-bottom-style: solid; border-bottom-width: 3; border-bottom-color: currentcolor; ",
            "border-left-style: solid; border-left-width: 3; border-left-color: currentcolor; ",
            "padding: 1em; background-color: white; color: black",
        ),
    ),
    ("dialog:not([open])", "display: none"),
    ("dialog:modal", "position: fixed; overflow: auto; top: 0; bottom: 0"),
];

/// The selectors of [`DIALOG_USER_AGENT_RULES`], in order, as values: the
/// selector parser exists only at macro time. A unit test checks that they
/// equal the parsed text.
fn dialog_user_agent_selectors() -> [Selector; 3] {
    let dialog = || SimpleSelector::Tag("dialog".to_owned());
    let compound = |simple: Vec<SimpleSelector>| Selector {
        subject: Compound { simple },
        ancestors: Vec::new(),
    };
    let open = compound(vec![SimpleSelector::Attribute(AttributeSelector {
        name: "open".to_owned(),
        matcher: AttributeMatcher::Present,
        case_insensitive: false,
    })]);
    [
        compound(vec![dialog()]),
        compound(vec![
            dialog(),
            SimpleSelector::PseudoClass(PseudoClass::Not(vec![open])),
        ]),
        compound(vec![
            dialog(),
            SimpleSelector::PseudoClass(PseudoClass::Modal),
        ]),
    ]
}

/// [`DIALOG_USER_AGENT_RULES`] as a user agent sheet. Every declaration
/// must parse: one the style parser drops is an error, not a rule that
/// silently does less.
pub(crate) fn dialog_user_agent_sheet() -> Result<StyleSheet, String> {
    let mut sheet = StyleSheet::new(CascadeOrigin::UserAgent);
    let selectors = dialog_user_agent_selectors();
    for (index, ((_, block), selector)) in DIALOG_USER_AGENT_RULES.iter().zip(selectors).enumerate()
    {
        let selectors = vec![selector];
        let parsed = parse_style_attribute(block);
        if let Some(dropped) = parsed.dropped.first() {
            return Err(format!("`{}`: {}", dropped.text, dropped.reason));
        }
        let mut declarations: Vec<Declaration> = parsed
            .declarations
            .into_iter()
            .map(|declaration| Declaration {
                property: declaration.property,
                value: declaration.value,
            })
            .collect();
        if index == 0 {
            for property in [StyleProperty::Width, StyleProperty::Height] {
                declarations.push(Declaration {
                    property,
                    value: LengthValue::Computed(Length::fit_content()).into(),
                });
            }
        }
        sheet.push(StyleRule::new(
            selectors,
            declarations.into_iter().collect::<Style>(),
        ));
    }
    Ok(sheet)
}

/// Per-dialog state that is not an attribute.
#[derive(Debug, Default, Clone)]
pub(crate) struct DialogState {
    /// Blink's `previously_focused_element_`, recorded by the focusing steps.
    previously_focused: Option<NodeHandle>,
    return_value: String,
}

/// The document's top layer and its dialogs.
#[derive(Debug, Default)]
pub(crate) struct TopLayer {
    /// Bottom to top. Today only modal dialogs enter it.
    pub(crate) elements: Vec<NodeHandle>,
    pub(crate) dialogs: HashMap<u32, DialogState>,
    /// Whether the user agent sheet has been adopted: on the first dialog.
    pub(crate) sheet_adopted: bool,
}

impl Engine {
    /// Whether `handle` is a `dialog` element.
    pub fn is_dialog(&self, handle: NodeHandle) -> Result<bool, EngineError> {
        Ok(self.local_name(handle)? == Some("dialog"))
    }

    fn require_dialog(&self, handle: NodeHandle) -> Result<(), EngineError> {
        if self.is_dialog(handle)? {
            Ok(())
        } else {
            Err(EngineError::InvalidInput("not a dialog"))
        }
    }

    /// Adopt the `dialog` user agent rules once this document has a dialog.
    pub(crate) fn note_dialog_created(&mut self) -> Result<(), EngineError> {
        if self.top_layer.sheet_adopted {
            return Ok(());
        }
        let sheet = dialog_user_agent_sheet()
            .map_err(|_| EngineError::InvalidInput("the dialog user agent sheet does not parse"))?;
        self.adopt_style_sheet(&sheet)
            .map_err(|_| EngineError::InvalidInput("the dialog user agent sheet is rejected"))?;
        self.top_layer.sheet_adopted = true;
        Ok(())
    }

    /// Whether the dialog has an `open` attribute (`HTMLDialogElement::
    /// IsOpen`).
    pub fn is_dialog_open(&self, handle: NodeHandle) -> Result<bool, EngineError> {
        self.require_dialog(handle)?;
        Ok(self.attribute(handle, "open")?.is_some())
    }

    /// Whether the dialog is modal: shown with [`Engine::show_modal`] and
    /// neither closed nor removed since. `:modal` matches it.
    pub fn is_modal_dialog(&self, handle: NodeHandle) -> Result<bool, EngineError> {
        self.resolve(handle)?;
        Ok(self.top_layer.elements.contains(&handle))
    }

    /// The top layer, bottom to top.
    pub fn top_layer(&self) -> &[NodeHandle] {
        &self.top_layer.elements
    }

    /// Blink's `Document::ActiveModalDialog`: the topmost modal dialog.
    pub fn active_modal_dialog(&self) -> Option<NodeHandle> {
        self.top_layer.elements.last().copied()
    }

    /// The dialog's `returnValue`.
    pub fn dialog_return_value(&self, handle: NodeHandle) -> Result<&str, EngineError> {
        self.require_dialog(handle)?;
        Ok(self
            .top_layer
            .dialogs
            .get(&handle.index)
            .map_or("", |state| state.return_value.as_str()))
    }

    /// Set the dialog's `returnValue`.
    pub fn set_dialog_return_value(
        &mut self,
        handle: NodeHandle,
        value: &str,
    ) -> Result<(), EngineError> {
        self.require_dialog(handle)?;
        self.top_layer
            .dialogs
            .entry(handle.index)
            .or_default()
            .return_value = value.to_owned();
        Ok(())
    }

    /// `HTMLDialogElement::show`, after its `beforetoggle` event: open the
    /// dialog as non-modal and run the focusing steps. A dialog that is
    /// already open does nothing, unless it is modal, which fails with
    /// [`EngineError::InvalidState`].
    pub fn show_dialog(&mut self, handle: NodeHandle) -> Result<(), EngineError> {
        if self.is_dialog_open(handle)? {
            if self.is_modal_dialog(handle)? {
                return Err(EngineError::InvalidState(SHOW_ON_MODAL));
            }
            return Ok(());
        }
        self.set_attribute(handle, "open", "")?;
        self.dialog_focusing_steps(handle)
    }

    /// `HTMLDialogElement::showModal`, after its `beforetoggle` event: put
    /// the dialog at the top of the top layer, make it modal and everything
    /// outside it inert, open it and run the focusing steps.
    ///
    /// A modal dialog that is already open does nothing. An open non-modal
    /// dialog, or one not in the document, fails with
    /// [`EngineError::InvalidState`]; [`Engine::check_show_modal`] makes the
    /// same checks without changing anything, for a caller that dispatches
    /// `beforetoggle` first.
    pub fn show_modal(&mut self, handle: NodeHandle) -> Result<(), EngineError> {
        if !self.check_show_modal(handle)? {
            return Ok(());
        }
        self.top_layer.elements.retain(|element| *element != handle);
        self.top_layer.elements.push(handle);
        self.note_top_layer_change();
        self.set_attribute(handle, "open", "")?;
        self.dialog_focusing_steps(handle)
    }

    /// The checks `showModal()` makes before its `beforetoggle` event:
    /// `Ok(false)` when the dialog is already open as a modal dialog, an
    /// [`EngineError::InvalidState`] error when it cannot open, and
    /// `Ok(true)` otherwise.
    pub fn check_show_modal(&self, handle: NodeHandle) -> Result<bool, EngineError> {
        if self.is_dialog_open(handle)? {
            if self.is_modal_dialog(handle)? {
                return Ok(false);
            }
            return Err(EngineError::InvalidState(SHOW_MODAL_ON_NON_MODAL));
        }
        if !self.is_connected(handle)? {
            return Err(EngineError::InvalidState(SHOW_MODAL_DISCONNECTED));
        }
        Ok(true)
    }

    /// `HTMLDialogElement::close`, after its `beforetoggle` event: remove
    /// `open`, take the dialog out of the top layer, set its `returnValue`
    /// when `return_value` is given, and restore focus. Returns false, doing
    /// nothing, when the dialog is not open. Queuing `close` is the caller's.
    ///
    /// Focus goes back, without scrolling, to the element focused before the
    /// dialog opened, when the dialog was modal or focus is inside it.
    pub fn close_dialog(
        &mut self,
        handle: NodeHandle,
        return_value: Option<&str>,
    ) -> Result<bool, EngineError> {
        if !self.is_dialog_open(handle)? {
            return Ok(false);
        }
        self.remove_attribute(handle, "open")?;
        let was_modal = self.is_modal_dialog(handle)?;
        if was_modal {
            self.top_layer.elements.retain(|element| *element != handle);
            self.note_top_layer_change();
        }
        let state = self.top_layer.dialogs.entry(handle.index).or_default();
        if let Some(value) = return_value {
            state.return_value = value.to_owned();
        }
        if let Some(previous) = state.previously_focused.take() {
            let focus_inside = self
                .focused
                .is_some_and(|focused| self.is_inclusive_ancestor(handle, focused));
            if (was_modal || focus_inside) && self.resolve(previous).is_ok() {
                // Blink's `Element::Focus` does nothing for an element that
                // cannot take focus.
                match self.focus_with_origin(previous, FocusOrigin::Script) {
                    Ok(_) | Err(EngineError::NotFocusable) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(true)
    }

    /// `HTMLDialogElement::SetFocusForDialogLegacy`: focus the first
    /// autofocus focusable descendant, else the first focusable one, else
    /// the dialog; descendants of nested dialogs are skipped. When the
    /// chosen element cannot take focus, focus is cleared.
    fn dialog_focusing_steps(&mut self, dialog: NodeHandle) -> Result<(), EngineError> {
        if !self.is_connected(dialog)? {
            return Ok(());
        }
        self.top_layer
            .dialogs
            .entry(dialog.index)
            .or_default()
            .previously_focused = self.focused;
        // Focusability reads style and, for scrollers, layout.
        self.update()?;
        let mut control = None;
        let mut stack: Vec<NodeHandle> = self.children(dialog)?.into_iter().rev().collect();
        while let Some(node) = stack.pop() {
            if !self.is_dialog(node)? {
                stack.extend(self.children(node)?.into_iter().rev());
            }
            if self.local_name(node)?.is_none() {
                continue;
            }
            let focusable = self.is_focusable(node);
            if focusable && self.attribute(node, "autofocus")?.is_some() {
                control = Some(node);
                break;
            }
            if control.is_none() && focusable {
                control = Some(node);
            }
        }
        let control = control.unwrap_or(dialog);
        if self.is_focusable(control) {
            self.focus_with_origin(control, FocusOrigin::Script)?;
        } else if let Some(focused) = self.focused {
            self.blur(focused)?;
        }
        Ok(())
    }

    /// The root of the part of the document that is not inert: the topmost
    /// modal dialog, else the modal root, if any.
    pub(crate) fn inert_scope(&self) -> Option<NodeHandle> {
        self.active_modal_dialog().or(self.modal_root)
    }

    /// Whether `handle` is inert: a modal dialog or modal root is active and
    /// `handle` is outside it (HTML's "blocked by a modal dialog").
    pub fn is_inert(&self, handle: NodeHandle) -> bool {
        let Some(scope) = self.inert_scope() else {
            return false;
        };
        !self.is_inclusive_ancestor(scope, self.public_event_target(handle))
    }

    /// The top layer changed: `:modal`, inertness, the hit-test order and
    /// the accessibility tree follow it.
    fn note_top_layer_change(&mut self) {
        self.note_selector_state_change();
        self.dirty.hit_test = true;
        self.mark_dirty(InvalidationClass::Accessibility);
    }

    /// A removed subtree leaves the top layer, as Blink's
    /// `HTMLDialogElement::RemovedFrom` does.
    pub(crate) fn note_top_layer_removal(&mut self, handles: &[NodeHandle]) {
        let before = self.top_layer.elements.len();
        self.top_layer
            .elements
            .retain(|element| !handles.contains(element));
        if self.top_layer.elements.len() != before {
            self.note_top_layer_change();
        }
    }

    /// A destroyed node's slot forgets its dialog state.
    pub(crate) fn forget_dialog_state(&mut self, index: u32) {
        self.top_layer.dialogs.remove(&index);
    }
}

#[cfg(test)]
mod tests {
    use div_style::selector_parser::parse_selector_list;

    #[test]
    fn the_user_agent_sheet_parses_completely() {
        if let Err(error) = super::dialog_user_agent_sheet() {
            panic!("{error}");
        }
    }

    #[test]
    fn the_user_agent_selectors_are_the_rules_text() {
        let built = super::dialog_user_agent_selectors();
        assert_eq!(built.len(), super::DIALOG_USER_AGENT_RULES.len());
        for ((text, _), selector) in super::DIALOG_USER_AGENT_RULES.iter().zip(built) {
            assert_eq!(
                parse_selector_list(text, None).unwrap(),
                vec![selector],
                "{text}"
            );
        }
    }
}
