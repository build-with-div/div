//! Render the components reference fixtures with Div.
//!
//!     div-components-render list
//!     div-components-render render <fixture-id> <out.png> [--viewport WxH] [--scale S]
//!     div-components-render geometry <fixture-id> [--viewport WxH] [--scale S]
//!     div-components-render trace <traces.json> <trace-id>
//!
//! `geometry` prints, as JSON, the border box of the fixture's root (the
//! page's `<main>`) and of every element inside it, depth first, in CSS
//! pixels. The harness compares it with Chromium's `getBoundingClientRect`
//! to tell layout differences from paint differences.
//!
//! 	race runs one input script of 	ests/fidelity/components/traces.json
//! on a trace fixture at 800x600 and prints, as JSON, the state of every
//! element with a data-slot before the first step and after each one. The
//! harness compares it with Radix running in Chromium.
//!
//! Each fixture ID matches a page built by `tests/fidelity/components/reference/build.mjs`
//! from the pinned shadcn/ui sources. A page is `<body>` with shadcn's
//! `bg-background text-foreground`, holding `<main class="p-4">` with the
//! fixture inside; this binary builds the same tree with the public `divui`
//! crate and renders it with Chromium's Linux LCD raster configuration.

use div::prelude::*;
use div_components::primitives::{ActivationMode, Key, TabsOptions, TabsState};
use div_components::{
    register_fonts, with_theme, Badge, BadgeProps, BadgeVariant, Button, ButtonProps, ButtonSize,
    ButtonVariant, Card, CardAction, CardActionProps, CardContent, CardContentProps,
    CardDescription, CardDescriptionProps, CardFooter, CardFooterProps, CardHeader,
    CardHeaderProps, CardProps, CardTitle, CardTitleProps, Label, LabelProps, Orientation,
    Separator, SeparatorProps, Tabs, TabsContent, TabsContentProps, TabsList, TabsListProps,
    TabsProps, TabsTrigger, TabsTriggerProps, Theme,
};

type Fixture = fn() -> ViewNode;

const THEMES: [(&str, Theme); 2] = [("light", Theme::LIGHT), ("dark", Theme::DARK)];

/// Fixtures in the order of `tests/fidelity/components/reference/fixtures.tsx`, with
/// whether each draws text. Fixtures without text skip registering the
/// bundled fonts, so a font-loading failure cannot hide their results.
const FIXTURES: &[(&str, Fixture, bool)] = &[
    ("font-geist", font_geist, true),
    ("font-geist-mono", font_geist_mono, true),
    ("font-inter", font_inter, true),
    ("separator-horizontal", separator_horizontal, false),
    ("separator-vertical", separator_vertical, false),
    ("button-default", button_default, true),
    ("button-destructive", button_destructive, true),
    ("button-outline", button_outline, true),
    ("button-secondary", button_secondary, true),
    ("button-ghost", button_ghost, true),
    ("button-link", button_link, true),
    ("button-size-xs", button_size_xs, true),
    ("button-size-sm", button_size_sm, true),
    ("button-size-lg", button_size_lg, true),
    ("button-disabled", button_disabled, true),
    ("badge-default", badge_default, true),
    ("badge-secondary", badge_secondary, true),
    ("badge-destructive", badge_destructive, true),
    ("badge-outline", badge_outline, true),
    ("badge-ghost", badge_ghost, true),
    ("badge-link", badge_link, true),
    ("card-basic", card_basic, true),
    ("card-action", card_action, true),
    ("label-basic", label_basic, true),
    ("tabs-default", tabs_default, true),
    ("tabs-vertical", tabs_vertical, true),
    ("tabs-password", tabs_password, true),
];

/// Fixtures of `traceFixtures` in fixtures.tsx, which `trace` runs.
const TRACE_FIXTURES: &[(&str, Fixture)] = &[
    ("tabs-default", tabs_default),
    ("tabs-vertical", tabs_vertical),
    ("tabs-manual", tabs_manual),
];

// `FontSpecimen` in fixtures.tsx: `flex flex-col gap-1 text-sm` and a font
// family, holding three lines at weights 400, 500 (`font-medium`) and 600
// (`font-semibold`). Tailwind's `text-sm` is 0.875rem with line-height
// calc(1.25 / 0.875), written as the number 1.4285714 because `line-height`
// does not accept `calc()` in Div yet.

fn font_geist() -> ViewNode {
    view! {
        <div
            style:display="flex"
            style:flex-direction="column"
            style:gap="4px"
            style:font-size="14px"
            style:line-height="1.4285714"
            style:font-family="Geist"
        >
            <p>"Sphinx of black quartz, judge my vow."</p>
            <p style:font-weight="500">"Pack my box with five dozen jugs."</p>
            <p style:font-weight="600">"0123456789 +-=*/ &@#%?!"</p>
        </div>
    }
}

fn font_geist_mono() -> ViewNode {
    view! {
        <div
            style:display="flex"
            style:flex-direction="column"
            style:gap="4px"
            style:font-size="14px"
            style:line-height="1.4285714"
            style:font-family="\"Geist Mono\""
        >
            <p>"Sphinx of black quartz, judge my vow."</p>
            <p style:font-weight="500">"Pack my box with five dozen jugs."</p>
            <p style:font-weight="600">"0123456789 +-=*/ &@#%?!"</p>
        </div>
    }
}

fn font_inter() -> ViewNode {
    view! {
        <div
            style:display="flex"
            style:flex-direction="column"
            style:gap="4px"
            style:font-size="14px"
            style:line-height="1.4285714"
            style:font-family="Inter"
        >
            <p>"Sphinx of black quartz, judge my vow."</p>
            <p style:font-weight="500">"Pack my box with five dozen jugs."</p>
            <p style:font-weight="600">"0123456789 +-=*/ &@#%?!"</p>
        </div>
    }
}

fn separator_horizontal() -> ViewNode {
    view! {
        <div style:width="256px">
            <Separator orientation=Orientation::Horizontal />
        </div>
    }
}

fn separator_vertical() -> ViewNode {
    view! {
        <div style:display="flex" style:height="32px">
            <Separator orientation=Orientation::Vertical />
        </div>
    }
}

// Buttons and badges sit in a `flex` row (`Row` in fixtures.tsx).

macro_rules! button_fixture {
    ($name:ident, $variant:ident, $size:ident, $disabled:expr, $text:literal) => {
        fn $name() -> ViewNode {
            view! {
                <div style:display="flex">
                    <Button
                        variant={ButtonVariant::$variant}
                        size={ButtonSize::$size}
                        disabled={$disabled}
                    >
                        $text
                    </Button>
                </div>
            }
        }
    };
}

button_fixture!(button_default, Default, Default, false, "Button");
button_fixture!(
    button_destructive,
    Destructive,
    Default,
    false,
    "Destructive"
);
button_fixture!(button_outline, Outline, Default, false, "Outline");
button_fixture!(button_secondary, Secondary, Default, false, "Secondary");
button_fixture!(button_ghost, Ghost, Default, false, "Ghost");
button_fixture!(button_link, Link, Default, false, "Link");
button_fixture!(button_size_xs, Default, Xs, false, "Extra small");
button_fixture!(button_size_sm, Default, Sm, false, "Small");
button_fixture!(button_size_lg, Default, Lg, false, "Large");
button_fixture!(button_disabled, Default, Default, true, "Disabled");

macro_rules! badge_fixture {
    ($name:ident, $variant:ident, $text:literal) => {
        fn $name() -> ViewNode {
            view! {
                <div style:display="flex">
                    <Badge variant={BadgeVariant::$variant}>$text</Badge>
                </div>
            }
        }
    };
}

badge_fixture!(badge_default, Default, "Badge");
badge_fixture!(badge_secondary, Secondary, "Secondary");
badge_fixture!(badge_destructive, Destructive, "Destructive");
badge_fixture!(badge_outline, Outline, "Outline");
badge_fixture!(badge_ghost, Ghost, "Ghost");
badge_fixture!(badge_link, Link, "Link");

/// `className="w-full max-w-sm"` on the example card.
const EXAMPLE_CARD: &[(&str, &str)] = &[("width", "100%"), ("max-width", "384px")];

fn card_basic() -> ViewNode {
    view! {
        <Card css={EXAMPLE_CARD}>
            <CardHeader>
                <CardTitle>"Card Title"</CardTitle>
                <CardDescription>"Card Description"</CardDescription>
            </CardHeader>
            <CardContent>
                <p>"Card Content"</p>
            </CardContent>
            <CardFooter>
                <p>"Card Footer"</p>
            </CardFooter>
        </Card>
    }
}

fn card_action() -> ViewNode {
    view! {
        <Card css={EXAMPLE_CARD}>
            <CardHeader>
                <CardTitle>"Card Title"</CardTitle>
                <CardDescription>"Card Description"</CardDescription>
                <CardAction>"Card Action"</CardAction>
            </CardHeader>
            <CardContent>
                <p>"Card Content"</p>
            </CardContent>
            <CardFooter>
                <p>"Card Footer"</p>
            </CardFooter>
        </Card>
    }
}

fn label_basic() -> ViewNode {
    view! {
        <Label>"Accept terms and conditions"</Label>
    }
}

// `TabsExample` in fixtures.tsx: three tabs, `selected` selected and the
// third disabled, each panel holding one `text-sm` line. Its line height is
// written as 20px, its computed value: the number 1.4285714 used above for
// the font specimens gives 19.9999996px, which rounds the baseline a pixel
// differently here.

fn tabs_example(
    orientation: Orientation,
    activation_mode: ActivationMode,
    selected: &str,
) -> ViewNode {
    let tabs = TabsState::new(TabsOptions {
        default_value: Some(selected.to_owned()),
        orientation,
        activation_mode,
        ..TabsOptions::default()
    });
    view! {
        <Tabs state={tabs.clone()}>
            <TabsList state={tabs.clone()}>
                <TabsTrigger state={tabs.clone()} value={"account"} disabled={false}>
                    "Account"
                </TabsTrigger>
                <TabsTrigger state={tabs.clone()} value={"password"} disabled={false}>
                    "Password"
                </TabsTrigger>
                <TabsTrigger state={tabs.clone()} value={"billing"} disabled={true}>
                    "Billing"
                </TabsTrigger>
            </TabsList>
            <TabsContent state={tabs.clone()} value={"account"}>
                <p style:font-size="14px" style:line-height="20px">
                    "Make changes to your account here."
                </p>
            </TabsContent>
            <TabsContent state={tabs.clone()} value={"password"}>
                <p style:font-size="14px" style:line-height="20px">
                    "Change your password here."
                </p>
            </TabsContent>
            <TabsContent state={tabs.clone()} value={"billing"}>
                <p style:font-size="14px" style:line-height="20px">
                    "Billing is not available."
                </p>
            </TabsContent>
        </Tabs>
    }
}

fn tabs_default() -> ViewNode {
    tabs_example(
        Orientation::Horizontal,
        ActivationMode::Automatic,
        "account",
    )
}

fn tabs_vertical() -> ViewNode {
    tabs_example(Orientation::Vertical, ActivationMode::Automatic, "account")
}

fn tabs_password() -> ViewNode {
    tabs_example(
        Orientation::Horizontal,
        ActivationMode::Automatic,
        "password",
    )
}

fn tabs_manual() -> ViewNode {
    tabs_example(Orientation::Horizontal, ActivationMode::Manual, "account")
}

/// Mounts a fixture's view; `view!` embeds child views through components.
#[allow(non_snake_case)]
#[component]
fn FixtureView(build: Fixture) -> ViewNode {
    build()
}

fn fixture_ids() -> Vec<String> {
    FIXTURES
        .iter()
        .flat_map(|(id, _, _)| THEMES.iter().map(move |(theme, _)| format!("{id}-{theme}")))
        .collect()
}

fn find_fixture(id: &str) -> Option<(Fixture, bool, Theme)> {
    FIXTURES.iter().find_map(|(base, fixture, text)| {
        THEMES.iter().find_map(|(name, theme)| {
            (id.strip_prefix(base)? == format!("-{name}")).then_some((*fixture, *text, *theme))
        })
    })
}

fn render(
    id: &str,
    output: &str,
    width: f64,
    height: f64,
    scale: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = mount_fixture(id, width, height, scale)?;
    app.render_png_to(0.0, output)?;
    Ok(())
}

/// The border boxes of the fixture root and its descendant elements.
fn geometry(
    id: &str,
    width: f64,
    height: f64,
    scale: f64,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let app = mount_fixture(id, width, height, scale)?;
    app.document().update_all()?;
    let root = app
        .document()
        .body()
        .first_child()?
        .ok_or("the fixture root is missing")?;
    let mut boxes = Vec::new();
    let mut stack = vec![(root, 0usize)];
    while let Some((element, depth)) = stack.pop() {
        let rect = element.bounding_rect()?;
        boxes.push(serde_json::json!({
            "depth": depth,
            "slot": element.get_attribute("data-slot")?,
            "rect": rect.map(|r| [r.x, r.y, r.width, r.height]),
        }));
        let mut children = Vec::new();
        let mut child = element.first_child()?;
        while let Some(next) = child {
            child = next.next_sibling()?;
            children.push((next, depth + 1));
        }
        stack.extend(children.into_iter().rev());
    }
    Ok(serde_json::json!({ "fixture": id, "boxes": boxes }))
}

fn mount_fixture(
    id: &str,
    width: f64,
    height: f64,
    scale: f64,
) -> Result<HeadlessApp, Box<dyn std::error::Error>> {
    let (fixture, text, theme) = find_fixture(id).ok_or_else(|| format!("unknown fixture {id}"))?;
    mount_page(id, fixture, text, theme, width, height, scale)
}

/// A page with `fixture` in its `<main>`, as `build.mjs` writes it.
fn mount_page(
    id: &str,
    fixture: Fixture,
    text: bool,
    theme: Theme,
    width: f64,
    height: f64,
    scale: f64,
) -> Result<HeadlessApp, Box<dyn std::error::Error>> {
    let mut app = HeadlessApp::with_options(
        ViewportMetrics::from_logical_size(width, height, scale)?,
        EngineOptions {
            raster_configuration: RasterConfiguration::chromium_linux_lcd(),
        },
    )?;
    if text {
        register_fonts(app.document())?;
    }
    // `body { @apply bg-background text-foreground }` from shadcn's base layer;
    // Tailwind's preflight removes the body margin.
    let body = app.document().body();
    body.set_background_color(theme.background.color())?;
    body.set_color(theme.foreground.color())?;
    let fixture_id = id.to_owned();
    // Preflight gives `html` the `--font-sans` family (Geist) and
    // `line-height: 1.5`; the page has no text outside `<main>`, so setting
    // them on `<main>` instead inherits the same values.
    app.mount(move || {
        with_theme(theme, || {
            view! {
                <div
                    data-fixture={fixture_id}
                    style:padding="16px"
                    style:font-family="Geist"
                    style:line-height="1.5"
                >
                    <FixtureView build=fixture />
                </div>
            }
        })
    })?;
    Ok(app)
}

// ── Behavior traces ─────────────────────────────────────────────────────

type Json = serde_json::Value;

/// Run the trace `trace_id` of the traces file at `path`.
fn trace(path: &str, trace_id: &str) -> Result<Json, Box<dyn std::error::Error>> {
    let traces: Json = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let strings = |key: &str| -> Vec<String> {
        traces[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect()
    };
    let (attributes, references) = (strings("attributes"), strings("id_references"));
    let script = traces["traces"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|script| script["id"] == trace_id)
        .ok_or_else(|| format!("no trace {trace_id} in {path}"))?;
    let fixture_id = script["fixture"]
        .as_str()
        .ok_or("the trace has no fixture")?;
    let fixture = TRACE_FIXTURES
        .iter()
        .find_map(|(id, fixture)| (*id == fixture_id).then_some(*fixture))
        .ok_or_else(|| format!("no trace fixture {fixture_id}"))?;
    // The primitives spawn no tasks, so updating the document settles it.
    let app = mount_page(fixture_id, fixture, true, Theme::LIGHT, 800.0, 600.0, 1.0)?;
    app.document().update_all()?;
    let mut snapshots = vec![snapshot(app.document(), &attributes, &references)?];
    for step in script["steps"].as_array().into_iter().flatten() {
        perform(app.document(), step)?;
        app.document().update_all()?;
        snapshots.push(snapshot(app.document(), &attributes, &references)?);
    }
    Ok(serde_json::json!({
        "trace": trace_id,
        "fixture": fixture_id,
        "snapshots": snapshots,
    }))
}

/// The elements with a `data-slot` under the fixture root, in tree order,
/// each named `slot#k` for the k-th with that slot.
fn slotted(document: &Document) -> Result<Vec<(String, Element)>, Box<dyn std::error::Error>> {
    let root = document
        .body()
        .first_child()?
        .ok_or("the fixture root is missing")?;
    let mut counts = std::collections::HashMap::<String, usize>::new();
    let mut named = Vec::new();
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        if let Some(slot) = element.get_attribute("data-slot")? {
            let count = counts.entry(slot.clone()).or_default();
            named.push((format!("{slot}#{count}"), element.clone()));
            *count += 1;
        }
        let mut children = Vec::new();
        let mut child = element.first_child()?;
        while let Some(next) = child {
            child = next.next_sibling()?;
            children.push(next);
        }
        stack.extend(children.into_iter().rev());
    }
    Ok(named)
}

/// The state of the slotted elements and the focused element. The values
/// of `references` attributes are ids, recorded as the elements' names.
fn snapshot(
    document: &Document,
    attributes: &[String],
    references: &[String],
) -> Result<Json, Box<dyn std::error::Error>> {
    let named = slotted(document)?;
    let mut ids = std::collections::HashMap::new();
    for (name, element) in &named {
        if let Some(id) = element.get_attribute("id")? {
            ids.insert(id, name.clone());
        }
    }
    let mut elements = Vec::new();
    for (name, element) in &named {
        let mut recorded = serde_json::Map::new();
        for attribute in attributes {
            if let Some(value) = element.get_attribute(attribute)? {
                let value = if references.contains(attribute) {
                    value
                        .split_whitespace()
                        .map(|id| ids.get(id).map_or("?", String::as_str))
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    value
                };
                recorded.insert(attribute.clone(), Json::String(value));
            }
        }
        let mut children = 0;
        let mut child = element.first_child()?;
        while let Some(next) = child {
            children += 1;
            child = next.next_sibling()?;
        }
        elements.push(serde_json::json!({
            "name": name,
            "attributes": recorded,
            "children": children,
        }));
    }
    let focused = match document.focused_element()? {
        None => "body".to_owned(),
        Some(focused) if focused.is_same_node(&document.body())? => "body".to_owned(),
        Some(focused) => {
            let mut found = "unnamed".to_owned();
            for (name, element) in &named {
                if element.is_same_node(&focused)? {
                    found = name.clone();
                }
            }
            found
        }
    };
    Ok(serde_json::json!({ "focused": focused, "elements": elements }))
}

/// One step of a trace: `{"press": key, "shift": bool}` presses and
/// releases a key named as `KeyboardEvent.key`; `{"click": name}` clicks
/// the centre of a slotted element.
fn perform(document: &Document, step: &Json) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(name) = step["press"].as_str() {
        let key = Key::from_text(name).ok_or_else(|| format!("unknown key {name:?}"))?;
        let modifiers = if step["shift"].as_bool() == Some(true) {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        };
        let (code, text) = (key.code(), key.text());
        document.dispatch_key_input(KeyEventType::Down, code, text, text, modifiers)?;
        document.dispatch_key_input(KeyEventType::Up, code, text, None, modifiers)?;
        return Ok(());
    }
    if let Some(name) = step["click"].as_str() {
        document.update_all()?;
        let element = slotted(document)?
            .into_iter()
            .find_map(|(slotted, element)| (slotted == name).then_some(element))
            .ok_or_else(|| format!("no element {name}"))?;
        let rect = element.bounding_rect()?.ok_or("the element has no box")?;
        let (x, y) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        for kind in [
            MouseEventType::Move,
            MouseEventType::Down,
            MouseEventType::Up,
        ] {
            document.dispatch_mouse_event(kind, x, y, MouseButton::Left, Modifiers::NONE)?;
        }
        return Ok(());
    }
    Err(format!("unknown step {step}").into())
}

/// `[--viewport WxH] [--scale S]`, defaulting to the 800x600 profile at 1x.
fn parse_surface(args: &[String]) -> Result<(f64, f64, f64), Box<dyn std::error::Error>> {
    let (mut width, mut height, mut scale) = (800.0, 600.0, 1.0);
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--viewport" => (width, height) = parse_viewport(value)?,
            "--scale" => scale = value.parse()?,
            other => return Err(format!("unknown option {other}").into()),
        }
    }
    Ok((width, height, scale))
}

fn parse_viewport(value: &str) -> Result<(f64, f64), Box<dyn std::error::Error>> {
    let (width, height) = value
        .split_once('x')
        .ok_or("--viewport expects WIDTHxHEIGHT")?;
    Ok((width.parse()?, height.parse()?))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("list") => {
            for id in fixture_ids() {
                println!("{id}");
            }
            Ok(())
        }
        Some("render") => {
            let id = args.get(1).ok_or("missing fixture id")?;
            let output = args.get(2).ok_or("missing output path")?;
            let (width, height, scale) = parse_surface(&args[3..])?;
            render(id, output, width, height, scale)
        }
        Some("geometry") => {
            let id = args.get(1).ok_or("missing fixture id")?;
            let (width, height, scale) = parse_surface(&args[2..])?;
            println!("{}", geometry(id, width, height, scale)?);
            Ok(())
        }
        Some("trace") => {
            let path = args.get(1).ok_or("missing traces file")?;
            let id = args.get(2).ok_or("missing trace id")?;
            println!("{}", trace(path, id)?);
            Ok(())
        }
        _ => Err("usage: div-components-render list | render <id> <out.png> \
                  [--viewport WxH] [--scale S] | geometry <id> [--viewport WxH] [--scale S] \
                  | trace <traces.json> <trace-id>"
            .into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixture_id_resolves_to_its_theme() {
        let ids = fixture_ids();
        assert_eq!(ids.len(), FIXTURES.len() * THEMES.len());
        for id in &ids {
            let (_, _, theme) = find_fixture(id).expect(id);
            let expected = if id.ends_with("-dark") {
                Theme::DARK
            } else {
                Theme::LIGHT
            };
            assert_eq!(theme, expected);
        }
        assert!(find_fixture("separator-horizontal").is_none());
        assert!(find_fixture("separator-horizontal-dim").is_none());
    }

    #[test]
    fn fixture_ids_match_the_reference_manifest() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../reference/pages/manifest.json")).unwrap();
        let mut reference: Vec<String> = manifest["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|fixture| fixture["id"].as_str().unwrap().to_owned())
            .collect();
        let mut ours = fixture_ids();
        reference.sort();
        ours.sort();
        assert_eq!(ours, reference);
    }

    #[test]
    fn trace_fixture_ids_match_the_reference_manifest() {
        let manifest: Json =
            serde_json::from_str(include_str!("../../reference/pages/manifest.json")).unwrap();
        let mut reference: Vec<&str> = manifest["traces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|trace| trace["id"].as_str().unwrap())
            .collect();
        let mut ours: Vec<&str> = TRACE_FIXTURES.iter().map(|(id, _)| *id).collect();
        reference.sort();
        ours.sort();
        assert_eq!(ours, reference);
    }

    #[test]
    fn every_trace_records_a_snapshot_per_step() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../traces.json");
        let traces: Json = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for script in traces["traces"].as_array().unwrap() {
            let id = script["id"].as_str().unwrap();
            let result = trace(path, id).unwrap();
            assert_eq!(
                result["snapshots"].as_array().unwrap().len(),
                script["steps"].as_array().unwrap().len() + 1,
                "{id}"
            );
        }
    }

    #[test]
    fn tab_enters_the_list_at_the_selected_trigger() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../traces.json");
        let result = trace(path, "tabs-keyboard").unwrap();
        let snapshots = result["snapshots"].as_array().unwrap();
        let focus: Vec<&str> = snapshots
            .iter()
            .map(|snapshot| snapshot["focused"].as_str().unwrap())
            .collect();
        // Ready, Tab, Right, Right (wraps past the disabled trigger), Left,
        // End, Home, Down (cross axis), Right, Tab, Shift+Tab.
        assert_eq!(
            focus,
            [
                "body",
                "tabs-trigger#0",
                "tabs-trigger#1",
                "tabs-trigger#0",
                "tabs-trigger#1",
                "tabs-trigger#1",
                "tabs-trigger#0",
                "tabs-trigger#0",
                "tabs-trigger#1",
                "tabs-content#1",
                "tabs-trigger#1",
            ]
        );
        let last = &snapshots[9]["elements"];
        let panel = last
            .as_array()
            .unwrap()
            .iter()
            .find(|element| element["name"] == "tabs-content#1")
            .unwrap();
        assert_eq!(panel["attributes"]["data-state"], "active");
        assert_eq!(panel["attributes"]["aria-labelledby"], "tabs-trigger#1");
        assert_eq!(panel["children"], 1);
    }

    #[test]
    fn geometry_lists_the_fixture_tree_depth_first() {
        let value = geometry("card-action-light", 800.0, 600.0, 1.0).unwrap();
        let slots: Vec<(u64, Option<&str>)> = value["boxes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| (b["depth"].as_u64().unwrap(), b["slot"].as_str()))
            .collect();
        assert_eq!(
            slots,
            [
                (0, None),
                (1, Some("card")),
                (2, Some("card-header")),
                (3, Some("card-title")),
                (3, Some("card-description")),
                (3, Some("card-action")),
                (2, Some("card-content")),
                (3, None),
                (2, Some("card-footer")),
                (3, None),
            ]
        );
    }

    /// Preflight gives buttons `font: inherit` and `letter-spacing: inherit`;
    /// Div has no `inherit` literal, so its button must already inherit.
    #[test]
    fn buttons_inherit_the_page_font() {
        let app = mount_fixture("button-outline-dark", 800.0, 600.0, 1.0).unwrap();
        let row = app.document().body().first_child().unwrap().unwrap();
        let row = row.first_child().unwrap().unwrap();
        let button = row.first_child().unwrap().unwrap();
        assert_eq!(
            button.get_attribute("data-slot").unwrap().as_deref(),
            Some("button")
        );
        let (row, button) = (
            row.computed_style().unwrap(),
            button.computed_style().unwrap(),
        );
        assert_eq!(
            format!("{:?}", button.font_family()),
            format!("{:?}", row.font_family())
        );
        assert_eq!(button.letter_spacing(), row.letter_spacing());
        assert_eq!(
            format!("{:?}", button.font_style()),
            format!("{:?}", row.font_style())
        );
    }

    #[test]
    fn renders_every_fixture_at_the_requested_surface_size() {
        let directory = std::env::temp_dir().join("div-components-render-test");
        std::fs::create_dir_all(&directory).unwrap();
        for id in fixture_ids() {
            let path = directory.join(format!("{id}.png"));
            render(&id, path.to_str().unwrap(), 375.0, 667.0, 2.0).unwrap();
            let png = std::fs::read(&path).unwrap();
            // PNG IHDR: width and height are big-endian u32 at bytes 16..24.
            let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
            let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
            assert_eq!((width, height), (750, 1334), "{id}");
        }
    }
}
