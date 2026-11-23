//! The DOM and CSSOM surfaces of the generated tests, on the public `divui`
//! API. A value or member without an exact `divui` call stops the subtest.

use super::{Host, Res, Rt, Throw, Value};
use div::typed_style::parse_literal;
use div::typed_style::stylesheet::StyleSheet;
use div::{Element, StyleProperty};

fn dom_error(surface: &str, error: impl std::fmt::Debug) -> Throw {
    Throw::unsupported(format!("{surface} ({error:?})"))
}

impl Rt {
    /// Adopt a page's `<style>`.
    pub fn adopt(&self, sheet: &StyleSheet) -> Result<(), Throw> {
        self.document()
            .adopt_stylesheet(sheet)
            .map(|_| ())
            .map_err(|error| dom_error("markup:<style>", error))
    }

    /// `document.createElement(name)`: an HTML document lowercases the name.
    pub(super) fn create_element(&self, name: &Value) -> Result<Element, Throw> {
        let name = self.to_string(name)?.to_ascii_lowercase();
        Element::create(self.document(), &name)
            .map_err(|error| dom_error(&format!("document.createElement({name})"), error))
    }

    /// A member of `document`.
    pub(super) fn document_member(&self, key: &str) -> Res {
        match key {
            "body" => Ok(Value::Host(Host::Element(self.document().body()))),
            "createElement" => Ok(Value::function("createElement", |rt, _, args| {
                let name = args.into_iter().next().unwrap_or_default();
                Ok(Value::Host(Host::Element(rt.create_element(&name)?)))
            })),
            "getElementById" => Ok(Value::function("getElementById", |rt, _, args| {
                let id = rt.to_string(&args.into_iter().next().unwrap_or_default())?;
                let found = rt
                    .document()
                    .element_by_id(&id)
                    .map_err(|error| dom_error("document.getElementById", error))?;
                Ok(found.map_or(Value::Null, |element| Value::Host(Host::Element(element))))
            })),
            other => Err(Throw::unsupported(format!("surface:document.{other}"))),
        }
    }

    /// A member of an element.
    pub(super) fn element_member(&self, element: &Element, key: &str) -> Res {
        let element = element.clone();
        match key {
            "style" => Ok(Value::Host(Host::Style(element))),
            "addEventListener" => Ok(Value::function("addEventListener", move |rt, _, args| {
                rt.add_event_listener(&element, args)
            })),
            "remove" => Ok(Value::function("remove", move |_, _, _| {
                element
                    .remove()
                    .map_err(|error| dom_error("element.remove", error))?;
                Ok(Value::Undefined)
            })),
            "appendChild" => Ok(Value::function("appendChild", move |_, _, args| {
                let Some(Value::Host(Host::Element(child))) = args.into_iter().next() else {
                    return Err(Throw::unsupported("surface:appendChild(non-element)"));
                };
                element
                    .append_child(&child)
                    .map_err(|error| dom_error("element.appendChild", error))?;
                Ok(Value::Host(Host::Element(child)))
            })),
            other => Err(Throw::unsupported(format!("surface:element.{other}"))),
        }
    }

    /// `element.style.<key> = value`. A value `parse_literal` does not take
    /// is reported: CSSOM would ignore an invalid one, but this cannot tell
    /// an invalid value from a valid one Div does not parse yet.
    pub(super) fn set_style(
        &self,
        element: &Element,
        key: &str,
        value: &Value,
    ) -> Result<(), Throw> {
        let property = match key {
            "width" => StyleProperty::Width,
            "height" => StyleProperty::Height,
            "padding" => StyleProperty::Padding,
            "paddingLeft" => StyleProperty::PaddingLeft,
            "paddingTop" => StyleProperty::PaddingTop,
            "display" => StyleProperty::Display,
            "boxSizing" => StyleProperty::BoxSizing,
            "border" => StyleProperty::Border,
            "left" => StyleProperty::Left,
            "top" => StyleProperty::Top,
            "writingMode" => StyleProperty::WritingMode,
            other => return Err(Throw::unsupported(format!("surface:.style.{other}="))),
        };
        let text = self.to_string(value)?;
        let parsed = parse_literal(property, &text)
            .map_err(|_| Throw::unsupported(format!("style-value:{key}={text}")))?;
        element
            .set_property(property, parsed)
            .map_err(|error| dom_error("element.style", error))
    }
}
