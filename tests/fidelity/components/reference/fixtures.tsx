// Reference fixtures: each renders one shadcn/ui component state.
//
// The same fixture IDs are implemented with the public `divui` crate in
// `tests/fidelity/components/render`. Each fixture is rendered in the light and
// the dark theme; the build appends `-light` or `-dark` to the ID.
//
// A fixture's markup sits inside the page's `<main data-fixture>` element,
// which has `p-4` (16px padding). Text uses the bundled fonts: Geist is the
// page's sans-serif face (`--font-sans`), as on the shadcn/ui site.
//
// Inline-level components (button, badge) sit in a `flex` row, as on the
// shadcn/ui site's examples, so each is a flex item rather than an inline
// box on a line of text.

import * as React from "react"

import { Badge } from "./shadcn/ui/badge"
import { Button } from "./shadcn/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "./shadcn/ui/card"
import { Checkbox } from "./shadcn/ui/checkbox"
import { Input } from "./shadcn/ui/input"
import { Label } from "./shadcn/ui/label"
import { Separator } from "./shadcn/ui/separator"
import { Switch } from "./shadcn/ui/switch"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./shadcn/ui/tabs"

export type Fixture = {
  id: string
  component: string
  description: string
  render: () => React.ReactElement
}

// Three `text-sm` lines at the weights shadcn's components use (400, 500,
// 600), each short enough not to wrap at the 375px profile in any face.
function FontSpecimen({ className }: { className: string }) {
  return (
    <div className={`flex flex-col gap-1 text-sm ${className}`}>
      <p>Sphinx of black quartz, judge my vow.</p>
      <p className="font-medium">Pack my box with five dozen jugs.</p>
      <p className="font-semibold">{"0123456789 +-=*/ &@#%?!"}</p>
    </div>
  )
}

type ButtonVariant =
  | "default"
  | "destructive"
  | "outline"
  | "secondary"
  | "ghost"
  | "link"

// Every variant at the default size. The icon sizes need a Lucide icon,
// which Div cannot draw yet, so they are not fixtures.
const buttonVariants: [ButtonVariant, string][] = [
  ["default", "Button"],
  ["destructive", "Destructive"],
  ["outline", "Outline"],
  ["secondary", "Secondary"],
  ["ghost", "Ghost"],
  ["link", "Link"],
]

const buttonSizes: ["xs" | "sm" | "lg", string][] = [
  ["xs", "Extra small"],
  ["sm", "Small"],
  ["lg", "Large"],
]

const badgeVariants: [ButtonVariant, string][] = [
  ["default", "Badge"],
  ["secondary", "Secondary"],
  ["destructive", "Destructive"],
  ["outline", "Outline"],
  ["ghost", "Ghost"],
  ["link", "Link"],
]

function Row({ children }: { children: React.ReactNode }) {
  return <div className="flex">{children}</div>
}

const buttonFixtures: Fixture[] = [
  ...buttonVariants.map(([variant, text]) => ({
    id: `button-${variant}`,
    component: "button",
    description: `Button, ${variant} variant, default size`,
    render: () => (
      <Row>
        <Button variant={variant}>{text}</Button>
      </Row>
    ),
  })),
  ...buttonSizes.map(([size, text]) => ({
    id: `button-size-${size}`,
    component: "button",
    description: `Button, default variant, size ${size}`,
    render: () => (
      <Row>
        <Button size={size}>{text}</Button>
      </Row>
    ),
  })),
  {
    id: "button-disabled",
    component: "button",
    description: "Disabled button, default variant (opacity 50%)",
    render: () => (
      <Row>
        <Button disabled>Disabled</Button>
      </Row>
    ),
  },
]

const badgeFixtures: Fixture[] = badgeVariants.map(([variant, text]) => ({
  id: `badge-${variant}`,
  component: "badge",
  description: `Badge, ${variant} variant`,
  render: () => (
    <Row>
      <Badge variant={variant}>{text}</Badge>
    </Row>
  ),
}))

// The card from the shadcn/ui docs' usage example, without and with an
// action in the header, at `w-full max-w-sm` (384px, or the 343px column of
// the 375px profile).
function ExampleCard({ action }: { action: boolean }) {
  return (
    <Card className="w-full max-w-sm">
      <CardHeader>
        <CardTitle>Card Title</CardTitle>
        <CardDescription>Card Description</CardDescription>
        {action ? <CardAction>Card Action</CardAction> : null}
      </CardHeader>
      <CardContent>
        <p>Card Content</p>
      </CardContent>
      <CardFooter>
        <p>Card Footer</p>
      </CardFooter>
    </Card>
  )
}

const cardFixtures: Fixture[] = [
  {
    id: "card-basic",
    component: "card",
    description: "Card with header (title, description), content and footer",
    render: () => <ExampleCard action={false} />,
  },
  {
    id: "card-action",
    component: "card",
    description: "Card whose header also has an action (two-column grid)",
    render: () => <ExampleCard action={true} />,
  },
]

const labelFixtures: Fixture[] = [
  {
    id: "label-basic",
    component: "label",
    description: "Label on its own",
    render: () => <Label>Accept terms and conditions</Label>,
  },
]

// Tabs with two enabled triggers and a disabled one, the first selected
// unless `defaultValue` says otherwise. Each panel holds one `text-sm` line.
// The line variant is not a fixture: its active indicator is an `::after`
// box, which Div cannot draw yet.
function TabsExample({
  orientation,
  activationMode,
  defaultValue = "account",
}: {
  orientation: "horizontal" | "vertical"
  activationMode: "automatic" | "manual"
  defaultValue?: string
}) {
  return (
    <Tabs
      defaultValue={defaultValue}
      orientation={orientation}
      activationMode={activationMode}
    >
      <TabsList>
        <TabsTrigger value="account">Account</TabsTrigger>
        <TabsTrigger value="password">Password</TabsTrigger>
        <TabsTrigger value="billing" disabled>
          Billing
        </TabsTrigger>
      </TabsList>
      <TabsContent value="account">
        <p className="text-sm">Make changes to your account here.</p>
      </TabsContent>
      <TabsContent value="password">
        <p className="text-sm">Change your password here.</p>
      </TabsContent>
      <TabsContent value="billing">
        <p className="text-sm">Billing is not available.</p>
      </TabsContent>
    </Tabs>
  )
}

const tabsFixtures: Fixture[] = [
  {
    id: "tabs-default",
    component: "tabs",
    description: "Horizontal tabs, the first of three selected, the third disabled",
    render: () => (
      <TabsExample orientation="horizontal" activationMode="automatic" />
    ),
  },
  {
    id: "tabs-vertical",
    component: "tabs",
    description: "Vertical tabs, the first of three selected, the third disabled",
    render: () => <TabsExample orientation="vertical" activationMode="automatic" />,
  },
  {
    id: "tabs-password",
    component: "tabs",
    description: "Horizontal tabs with the second selected",
    render: () => (
      <TabsExample
        orientation="horizontal"
        activationMode="automatic"
        defaultValue="password"
      />
    ),
  },
]

// Each switch state at both sizes, in a flex row like the button. Radix
// renders a hidden form input after the switch on the server, which the
// static pages keep; in the browser it is removed outside a `<form>`.
const switchStates: [string, string, { defaultChecked?: boolean; disabled?: boolean }][] = [
  ["unchecked", "unchecked", {}],
  ["checked", "checked", { defaultChecked: true }],
  ["disabled", "unchecked and disabled (opacity 50%)", { disabled: true }],
  [
    "disabled-checked",
    "checked and disabled (opacity 50%)",
    { defaultChecked: true, disabled: true },
  ],
]

const switchFixtures: Fixture[] = [
  ...switchStates.map(([state, description, props]) => ({
    id: `switch-${state}`,
    component: "switch",
    description: `Switch, default size, ${description}`,
    render: () => (
      <Row>
        <Switch {...props} />
      </Row>
    ),
  })),
  ...switchStates
    .filter(([state]) => !state.startsWith("disabled"))
    .map(([state, description, props]) => ({
      id: `switch-sm-${state}`,
      component: "switch",
      description: `Switch, size sm, ${description}`,
      render: () => (
        <Row>
          <Switch size="sm" {...props} />
        </Row>
      ),
    })),
]

// Each checkbox state, in a flex row like the switch. Radix renders a hidden
// form input after the checkbox on the server, which the static pages keep;
// in the browser it is removed outside a `<form>`. The indicator holds
// Lucide's check icon while the checkbox is checked.
const checkboxStates: [
  string,
  string,
  { defaultChecked?: boolean; disabled?: boolean; "aria-invalid"?: boolean },
][] = [
  ["unchecked", "unchecked", {}],
  ["checked", "checked", { defaultChecked: true }],
  ["disabled", "unchecked and disabled (opacity 50%)", { disabled: true }],
  [
    "disabled-checked",
    "checked and disabled (opacity 50%)",
    { defaultChecked: true, disabled: true },
  ],
  ["invalid", "unchecked with aria-invalid (destructive border)", { "aria-invalid": true }],
]

const checkboxFixtures: Fixture[] = checkboxStates.map(([state, description, props]) => ({
  id: `checkbox-${state}`,
  component: "checkbox",
  description: `Checkbox, ${description}`,
  render: () => (
    <Row>
      <Checkbox {...props} />
    </Row>
  ),
}))

// An email input, as in the shadcn/ui docs' demo, in a 256px wide column
// (`w-full` fills it). At 768px and wider `md:text-sm` sets 14px text.
const inputStates: [string, string, React.ComponentProps<"input">][] = [
  ["placeholder", "empty, showing its placeholder", { placeholder: "Email" }],
  ["value", "holding a value", { defaultValue: "name@example.com" }],
  [
    "disabled",
    "disabled, showing its placeholder (opacity 50%)",
    { placeholder: "Email", disabled: true },
  ],
  [
    "invalid",
    "aria-invalid, holding a value (destructive border and ring)",
    { defaultValue: "name@example", "aria-invalid": true },
  ],
]

const inputFixtures: Fixture[] = inputStates.map(([state, description, props]) => ({
  id: `input-${state}`,
  component: "input",
  description: `Email input, ${description}`,
  render: () => (
    <div className="w-64">
      <Input type="email" {...props} />
    </div>
  ),
}))

// Fixtures whose behavior the trace harness compares with Radix running in
// Chromium. Each gets an interactive page (`trace-<id>.html`) that renders
// it with React in the browser; manual activation looks the same as
// automatic, so it has no static page. The traces select the second tab
// themselves, so `tabs-password` has no trace page; they toggle the switch
// themselves, so only the unchecked and the disabled switch have one, and
// the same for the checkbox. The empty and the disabled input have one, for
// focus.
export const traceFixtures: Fixture[] = [
  ...tabsFixtures.filter((fixture) => fixture.id !== "tabs-password"),
  {
    id: "tabs-manual",
    component: "tabs",
    description: "Horizontal tabs with manual activation",
    render: () => <TabsExample orientation="horizontal" activationMode="manual" />,
  },
  ...switchFixtures.filter(
    (fixture) => fixture.id === "switch-unchecked" || fixture.id === "switch-disabled"
  ),
  ...inputFixtures.filter(
    (fixture) => fixture.id === "input-placeholder" || fixture.id === "input-disabled"
  ),
  ...checkboxFixtures.filter(
    (fixture) => fixture.id === "checkbox-unchecked" || fixture.id === "checkbox-disabled"
  ),
]

export const fixtures: Fixture[] = [
  {
    id: "font-geist",
    component: "font",
    description: "Geist (--font-sans) at text-sm, weights 400, 500 and 600",
    render: () => <FontSpecimen className="font-sans" />,
  },
  {
    id: "font-geist-mono",
    component: "font",
    description: "Geist Mono (--font-mono) at text-sm, weights 400, 500 and 600",
    render: () => <FontSpecimen className="font-mono" />,
  },
  {
    id: "font-inter",
    component: "font",
    description: "Inter (--font-inter) at text-sm, weights 400, 500 and 600",
    render: () => <FontSpecimen className="font-inter" />,
  },
  {
    id: "separator-horizontal",
    component: "separator",
    description: "Horizontal separator in a 256px wide column",
    render: () => (
      <div className="w-64">
        <Separator />
      </div>
    ),
  },
  {
    id: "separator-vertical",
    component: "separator",
    description: "Vertical separator in a 32px tall flex row",
    render: () => (
      <div className="flex h-8">
        <Separator orientation="vertical" />
      </div>
    ),
  },
  ...buttonFixtures,
  ...badgeFixtures,
  ...cardFixtures,
  ...labelFixtures,
  ...tabsFixtures,
  ...switchFixtures,
  ...inputFixtures,
  ...checkboxFixtures,
]
