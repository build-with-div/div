//! CSS math functions: `calc()` over numbers, lengths and percentages.
//!
//! This is a port of Blink's math expression tree, as Chromium 147 builds it
//! (`core/css/css_math_expression_node.cc`, `css_math_function_value.cc` and
//! `platform/geometry/calculation_value.cc`, BSD):
//!
//! - **Parsing** follows `CSSMathExpressionNodeParser`: `+` and `-` need
//!   whitespace on both sides, `*` and `/` bind tighter, and parentheses or
//!   a nested `calc()` group a term. A block left open at the end of the
//!   input is closed, as CSS syntax closes it.
//! - **Simplification** happens while parsing, in Blink's order and in
//!   double precision: two numeric literals are combined at once, a number
//!   distributes over a sum, and each sum or product is flattened so that
//!   literals of one unit combine. Absolute lengths of different units
//!   combine in pixels; relative lengths stay apart.
//! - **Types** follow CSS Typed OM, so `1px / 2px` is a number and
//!   `1px * 2px` is an intermediate result no property accepts.
//! - **Evaluation** keeps Blink's two paths: a number or a pure length is
//!   computed in double precision through the tree, and a length with a
//!   percentage is folded into pixels and percent in single precision.
//!
//! Only lengths, percentages and numbers are parsed. Angles, times,
//! frequencies, resolutions, `min()`, `max()`, `clamp()` and the other
//! math functions are not, so a value using them is rejected.

use crate::LineHeight;
use std::collections::HashMap;

/// Blink's `kMaxExpressionDepth`.
const MAX_DEPTH: u8 = 100;

/// The unit of a numeric literal inside a math function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
pub enum MathUnit {
    Number,
    Percent,
    Px,
    Cm,
    Mm,
    Q,
    In,
    Pt,
    Pc,
    Em,
    Rem,
    Ex,
    Ch,
    Lh,
    Vw,
    Vh,
    Vmin,
    Vmax,
}

impl MathUnit {
    fn from_css(unit: &str) -> Option<Self> {
        Some(match unit.to_ascii_lowercase().as_str() {
            "px" => Self::Px,
            "cm" => Self::Cm,
            "mm" => Self::Mm,
            "q" => Self::Q,
            "in" => Self::In,
            "pt" => Self::Pt,
            "pc" => Self::Pc,
            "em" => Self::Em,
            "rem" => Self::Rem,
            "ex" => Self::Ex,
            "ch" => Self::Ch,
            "lh" => Self::Lh,
            "vw" => Self::Vw,
            "vh" => Self::Vh,
            "vmin" => Self::Vmin,
            "vmax" => Self::Vmax,
            _ => return None,
        })
    }

    fn category(self) -> MathCategory {
        match self {
            Self::Number => MathCategory::Number,
            Self::Percent => MathCategory::Percent,
            _ => MathCategory::Length,
        }
    }

    /// CSS pixels per unit for the absolute lengths (CSS Values 4: 1in is
    /// 96px, 2.54cm, 72pt and 6pc; 1cm is 10mm and 40Q).
    fn absolute_px(self) -> Option<f64> {
        const PX_PER_CM: f64 = 96.0 / 2.54;
        Some(match self {
            Self::Px => 1.0,
            Self::Cm => PX_PER_CM,
            Self::Mm => PX_PER_CM / 10.0,
            Self::Q => PX_PER_CM / 40.0,
            Self::In => 96.0,
            Self::Pt => 96.0 / 72.0,
            Self::Pc => 96.0 / 6.0,
            _ => return None,
        })
    }
}

/// Blink's `CalculationResultCategory`, for the categories Div parses.
/// `LengthPercent` is Blink's `kCalcLengthFunction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
pub enum MathCategory {
    Number,
    Length,
    Percent,
    LengthPercent,
    /// A product of dimensions, such as `1px * 1px`, valid only inside a
    /// larger expression that cancels it.
    Intermediate,
}

/// A base type of a CSS numeric type. Blink also has angle, time,
/// frequency, resolution and flex, which Div does not parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
pub enum MathBaseType {
    Length,
    Percent,
}

/// Blink's `CSSMathType`: the power of each base type and the percent hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
pub struct MathType {
    pub valid: bool,
    pub length: i8,
    pub percent: i8,
    pub hint: Option<MathBaseType>,
}

impl MathType {
    const NUMBER: Self = Self {
        valid: true,
        length: 0,
        percent: 0,
        hint: None,
    };
    const INVALID: Self = Self {
        valid: false,
        length: 0,
        percent: 0,
        hint: None,
    };

    fn of_category(category: MathCategory) -> Self {
        match category {
            MathCategory::Number | MathCategory::Intermediate => Self::NUMBER,
            MathCategory::Length => Self {
                length: 1,
                ..Self::NUMBER
            },
            MathCategory::Percent => Self {
                percent: 1,
                ..Self::NUMBER
            },
            MathCategory::LengthPercent => Self {
                length: 1,
                hint: Some(MathBaseType::Length),
                ..Self::NUMBER
            },
        }
    }

    fn of_node(node: &MathNode) -> Self {
        match node {
            MathNode::Operation(operation) => operation.math_type,
            MathNode::Literal { unit, .. } => Self::of_category(unit.category()),
        }
    }

    /// `CSSMathType::Category`: `None` is Blink's `kCalcOther`.
    fn category(&self) -> Option<MathCategory> {
        if !self.valid {
            return None;
        }
        // Base types in Blink's order, percent last.
        let mut found = None;
        let mut sum: i16 = 0;
        for (base, power) in [
            (MathBaseType::Length, self.length),
            (MathBaseType::Percent, self.percent),
        ] {
            if power == 0 {
                continue;
            }
            if sum != 0 {
                if base != MathBaseType::Percent {
                    return Some(MathCategory::Intermediate);
                }
                sum += power as i16;
                break;
            }
            found = Some(base);
            sum += power as i16;
        }
        if sum == 0 {
            return Some(MathCategory::Number);
        }
        if sum != 1 {
            return Some(MathCategory::Intermediate);
        }
        if self.hint.is_some() {
            return (self.length != 0).then_some(MathCategory::LengthPercent);
        }
        Some(match found? {
            MathBaseType::Length => MathCategory::Length,
            MathBaseType::Percent => MathCategory::Percent,
        })
    }

    fn is_intermediate(&self) -> bool {
        self.valid && self.category() == Some(MathCategory::Intermediate)
    }

    fn apply_hint(&mut self, hint: MathBaseType) {
        if self.hint.is_some() {
            return;
        }
        self.hint = Some(hint);
        if hint != MathBaseType::Percent {
            self.length += self.percent;
            self.percent = 0;
        }
    }

    fn reconcile_hints(type1: &mut Self, type2: &mut Self) -> bool {
        if let (Some(a), Some(b)) = (type1.hint, type2.hint) {
            if a != b {
                return false;
            }
        }
        if let (Some(hint), None) = (type1.hint, type2.hint) {
            type2.apply_hint(hint);
        }
        if let (None, Some(hint)) = (type1.hint, type2.hint) {
            type1.apply_hint(hint);
        }
        true
    }

    fn add(mut type1: Self, mut type2: Self) -> Self {
        if !Self::reconcile_hints(&mut type1, &mut type2) {
            return Self::INVALID;
        }
        if type1 == type2 {
            return type1;
        }
        let contains_percent = type1.percent != 0 || type2.percent != 0;
        let contains_other = type1.length != 0 || type2.length != 0;
        if contains_percent && contains_other {
            // Each base type other than percent, as a provisional hint.
            let (mut provisional1, mut provisional2) = (type1, type2);
            provisional1.apply_hint(MathBaseType::Length);
            provisional2.apply_hint(MathBaseType::Length);
            if provisional1 == provisional2 {
                return provisional1;
            }
        }
        Self::INVALID
    }

    fn multiply(mut type1: Self, mut type2: Self) -> Self {
        if !Self::reconcile_hints(&mut type1, &mut type2) {
            return Self::INVALID;
        }
        type1.length += type2.length;
        type1.percent += type2.percent;
        type1
    }

    fn negate(self) -> Self {
        Self {
            length: -self.length,
            percent: -self.percent,
            ..self
        }
    }
}

/// An operator of a math expression. `Divide` only joins two terms while
/// parsing: Blink stores `a / b` as `a * (1 / b)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
pub enum MathOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Invert,
}

/// An operation node, Blink's `CSSMathExpressionOperation`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
pub struct MathOperation {
    pub operator: MathOperator,
    pub operands: Vec<MathNode>,
    pub category: MathCategory,
    pub math_type: MathType,
    /// Set on a parenthesized or nested `calc()` result, and on a
    /// distributed product, so that flattening a surrounding sum carries
    /// the sign of a subtraction into it.
    pub nested: bool,
    pub nested_intermediate: bool,
}

/// A node of a simplified math expression.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
pub enum MathNode {
    Literal { value: f64, unit: MathUnit },
    Operation(Box<MathOperation>),
}

impl MathNode {
    fn literal(value: f64, unit: MathUnit) -> Self {
        Self::Literal { value, unit }
    }

    fn category(&self) -> MathCategory {
        match self {
            Self::Literal { unit, .. } => unit.category(),
            Self::Operation(operation) => operation.category,
        }
    }

    fn is_literal(&self) -> bool {
        matches!(self, Self::Literal { .. })
    }

    fn literal_value(&self) -> f64 {
        match self {
            Self::Literal { value, .. } => *value,
            Self::Operation(_) => 0.0,
        }
    }

    fn literal_unit(&self) -> Option<MathUnit> {
        match self {
            Self::Literal { unit, .. } => Some(*unit),
            Self::Operation(_) => None,
        }
    }

    fn operation(&self) -> Option<&MathOperation> {
        match self {
            Self::Operation(operation) => Some(operation),
            Self::Literal { .. } => None,
        }
    }

    fn is_add_or_subtract(&self) -> bool {
        self.operation().is_some_and(|operation| {
            matches!(
                operation.operator,
                MathOperator::Add | MathOperator::Subtract
            )
        })
    }

    fn is_multiply(&self) -> bool {
        self.operation()
            .is_some_and(|operation| operation.operator == MathOperator::Multiply)
    }

    fn is_invert(&self) -> bool {
        self.operation()
            .is_some_and(|operation| operation.operator == MathOperator::Invert)
    }

    fn nested_intermediate(&self) -> bool {
        self.operation()
            .is_some_and(|operation| operation.nested_intermediate)
    }

    fn set_nested(&mut self) {
        if let Self::Operation(operation) = self {
            operation.nested = true;
        }
    }

    fn operation_node(
        operator: MathOperator,
        operands: Vec<MathNode>,
        category: MathCategory,
        math_type: MathType,
    ) -> Self {
        let nested_intermediate =
            math_type.is_intermediate() || operands.iter().any(Self::nested_intermediate);
        Self::Operation(Box::new(MathOperation {
            operator,
            operands,
            category,
            math_type,
            nested: false,
            nested_intermediate,
        }))
    }

    /// `Copy()`: a new node, without the nesting mark.
    fn copied(&self) -> Self {
        let mut node = self.clone();
        if let Self::Operation(operation) = &mut node {
            operation.nested = false;
        }
        node
    }
}

// ---- Construction and simplification (Blink's static constructors) ----

fn evaluate_operator(operands: &[f64], operator: MathOperator) -> f64 {
    if let Some(nan) = operands.iter().find(|value| value.is_nan()) {
        return *nan;
    }
    match operator {
        MathOperator::Add => operands[0] + operands[1],
        MathOperator::Subtract => operands[0] - operands[1],
        MathOperator::Multiply => operands[0] * operands[1],
        MathOperator::Divide => operands[0] / operands[1],
        MathOperator::Invert => 1.0 / operands[0],
    }
}

fn determine_type(left: &MathNode, right: &MathNode, operator: MathOperator) -> MathType {
    let (left, right) = (MathType::of_node(left), MathType::of_node(right));
    match operator {
        MathOperator::Add | MathOperator::Subtract => MathType::add(left, right),
        MathOperator::Multiply => MathType::multiply(left, right),
        MathOperator::Divide => MathType::multiply(left, right.negate()),
        MathOperator::Invert => MathType::INVALID,
    }
}

/// `CreateInvertFunction`.
fn create_invert(operand: MathNode) -> MathNode {
    if let MathNode::Literal { value, unit } = &operand {
        if unit.category() == MathCategory::Number && *value != 0.0 {
            return MathNode::literal(1.0 / value, MathUnit::Number);
        }
    }
    if operand.is_invert() {
        let MathNode::Operation(operation) = operand else {
            unreachable!()
        };
        return operation.operands[0].copied();
    }
    if operand.category() != MathCategory::Number {
        let math_type = MathType::of_node(&operand).negate();
        return MathNode::operation_node(
            MathOperator::Invert,
            vec![operand],
            MathCategory::Intermediate,
            math_type,
        );
    }
    MathNode::operation_node(
        MathOperator::Invert,
        vec![operand],
        MathCategory::Number,
        MathType::NUMBER,
    )
}

/// `CreateArithmeticOperation`.
fn create_arithmetic(
    left: MathNode,
    right: MathNode,
    mut operator: MathOperator,
) -> Option<MathNode> {
    let math_type = determine_type(&left, &right, operator);
    // Typed arithmetic is on in Chromium 147: the type decides the
    // category, so `1px / 1px` is a number.
    if !math_type.valid {
        return None;
    }
    let category = math_type.category()?;
    let mut right = right;
    if operator == MathOperator::Divide {
        let inverted = create_invert(right);
        if left.category() == MathCategory::Number
            && left.is_literal()
            && left.literal_value() == 1.0
        {
            return Some(inverted);
        }
        operator = MathOperator::Multiply;
        right = inverted;
    }
    Some(MathNode::operation_node(
        operator,
        vec![left, right],
        category,
        math_type,
    ))
}

/// `CanArithmeticOperationBeSimplified`.
fn can_be_simplified(left: &MathNode, right: &MathNode, operator: MathOperator) -> bool {
    if !left.is_literal() || !right.is_literal() {
        return false;
    }
    if matches!(operator, MathOperator::Multiply | MathOperator::Divide)
        && left.category() != MathCategory::Number
        && right.category() != MathCategory::Number
    {
        return false;
    }
    !determine_type(left, right, operator).is_intermediate()
}

fn all_operands_numeric(operation: &MathOperation) -> bool {
    operation.operands.iter().all(MathNode::is_literal)
}

/// `MaybeDistributeArithmeticOperation`.
fn maybe_distribute(
    left: &MathNode,
    right: &MathNode,
    operator: MathOperator,
) -> Option<Option<MathNode>> {
    if !matches!(operator, MathOperator::Multiply | MathOperator::Divide) {
        return None;
    }
    if left.category() == MathCategory::Intermediate
        || right.category() == MathCategory::Intermediate
    {
        return None;
    }
    let number_literal =
        |node: &MathNode| node.is_literal() && node.category() == MathCategory::Number;
    // (a + b) * n and (a + b) / n.
    if let Some(sum) = left.operation() {
        if left.is_add_or_subtract() && all_operands_numeric(sum) && number_literal(right) {
            let distributed = (|| {
                let new_left =
                    simplified_arithmetic(sum.operands[0].clone(), right.clone(), operator)?;
                let new_right =
                    simplified_arithmetic(sum.operands[1].clone(), right.clone(), operator)?;
                let mut node = simplified_arithmetic(new_left, new_right, sum.operator)?;
                node.set_nested();
                Some(node)
            })();
            return Some(distributed);
        }
    }
    // n * (a + b), but not n / (a + b).
    if let Some(sum) = right.operation() {
        if right.is_add_or_subtract()
            && all_operands_numeric(sum)
            && number_literal(left)
            && operator != MathOperator::Divide
        {
            let distributed = (|| {
                let new_right =
                    simplified_arithmetic(left.clone(), sum.operands[0].clone(), operator)?;
                let new_left =
                    simplified_arithmetic(left.clone(), sum.operands[1].clone(), operator)?;
                let mut node = simplified_arithmetic(new_right, new_left, sum.operator)?;
                node.set_nested();
                Some(node)
            })();
            return Some(distributed);
        }
    }
    None
}

/// `CreateArithmeticOperationSimplified`. `None` is a parse error.
fn simplified_arithmetic(
    left: MathNode,
    right: MathNode,
    operator: MathOperator,
) -> Option<MathNode> {
    if let Some(distributed) = maybe_distribute(&left, &right, operator) {
        return distributed;
    }
    if !can_be_simplified(&left, &right, operator) {
        return create_arithmetic(left, right, operator);
    }
    let (left_category, right_category) = (left.category(), right.category());
    let (left_value, right_value) = (left.literal_value(), right.literal_value());
    let (left_unit, right_unit) = (left.literal_unit()?, right.literal_unit()?);
    // Numbers.
    if left_category == MathCategory::Number && right_category == MathCategory::Number {
        return Some(MathNode::literal(
            evaluate_operator(&[left_value, right_value], operator),
            MathUnit::Number,
        ));
    }
    match operator {
        MathOperator::Add | MathOperator::Subtract => {
            if left_category == right_category {
                if left_unit == right_unit {
                    return Some(MathNode::literal(
                        evaluate_operator(&[left_value, right_value], operator),
                        left_unit,
                    ));
                }
                // Absolute lengths of different units combine in pixels.
                if let (Some(left_px), Some(right_px)) =
                    (left_unit.absolute_px(), right_unit.absolute_px())
                {
                    return Some(MathNode::literal(
                        evaluate_operator(
                            &[left_value * left_px, right_value * right_px],
                            operator,
                        ),
                        MathUnit::Px,
                    ));
                }
            }
        }
        _ => {
            if right_category != MathCategory::Number && operator == MathOperator::Divide {
                return None;
            }
            let (number, other_value, other_unit) = if left_category == MathCategory::Number {
                (left_value, right_value, right_unit)
            } else if right_category == MathCategory::Number {
                (right_value, left_value, left_unit)
            } else {
                return create_arithmetic(left, right, operator);
            };
            return Some(MathNode::literal(
                evaluate_operator(&[other_value, number], operator),
                other_unit,
            ));
        }
    }
    create_arithmetic(left, right, operator)
}

/// `MaybeChangeOperatorSignIfNesting`.
fn sign_in_nesting(nesting: bool, outer: MathOperator, current: MathOperator) -> MathOperator {
    match (nesting, outer, current) {
        (true, MathOperator::Subtract, MathOperator::Add) => MathOperator::Subtract,
        (true, MathOperator::Subtract, MathOperator::Subtract) => MathOperator::Add,
        _ => current,
    }
}

/// `TraverseNumericChildrenFromNode`: visits the leaves of a sum (or of a
/// product), left to right, with the sign each leaf carries.
fn traverse_children<'a>(
    root: &'a MathNode,
    operator: MathOperator,
    mut visit: impl FnMut(&'a MathNode, MathOperator),
) {
    let should_traverse = |node: &MathNode, operator: MathOperator| {
        node.operation().is_some()
            && !node.nested_intermediate()
            && if operator == MathOperator::Multiply {
                node.is_multiply()
            } else {
                node.is_add_or_subtract()
            }
    };
    let mut stack: Vec<(&'a MathNode, MathOperator, bool)> = Vec::new();
    let push = |stack: &mut Vec<(&'a MathNode, MathOperator, bool)>,
                node: &'a MathNode,
                operator: MathOperator,
                nesting: bool| {
        let operation = node.operation().expect("an operation");
        let nesting = nesting || operation.nested;
        let back = sign_in_nesting(nesting, operator, operation.operator);
        stack.push((&operation.operands[1], back, nesting));
        stack.push((&operation.operands[0], operator, nesting));
    };
    if !should_traverse(root, operator) {
        visit(root, operator);
        return;
    }
    push(&mut stack, root, operator, false);
    while let Some((node, operator, nesting)) = stack.pop() {
        if !should_traverse(node, operator) {
            visit(node, operator);
            continue;
        }
        push(&mut stack, node, operator, nesting);
    }
}

/// `MaybeSimplifySumOrProductNode`.
fn simplify_sum_or_product(root: MathNode) -> Option<MathNode> {
    let is_multiply = root.is_multiply();
    let start = if is_multiply {
        MathOperator::Multiply
    } else {
        MathOperator::Add
    };
    let mut combined: HashMap<MathUnit, f64> = HashMap::new();
    let mut children: Vec<(MathOperator, MathNode)> = Vec::new();
    traverse_children(&root, start, |node, operator| {
        if let MathNode::Literal { value, unit } = node {
            let value = if operator == MathOperator::Subtract {
                -value
            } else {
                *value
            };
            combined
                .entry(*unit)
                .and_modify(|total| {
                    if operator == MathOperator::Multiply {
                        *total *= value;
                    } else {
                        *total += value;
                    }
                })
                .or_insert(value);
        }
        children.push((operator, node.clone()));
    });
    let count = combined.len() + children.len();
    let mut used = Vec::new();
    let mut result: Option<MathNode> = None;
    for (operator, node) in children {
        // `MaybeReplaceNodeWithCombined`.
        let (operator, node) = match node {
            MathNode::Literal { unit, .. } => {
                let mut value = combined[&unit];
                let mut operator = operator;
                if !is_multiply {
                    operator = if value < 0.0 {
                        MathOperator::Subtract
                    } else {
                        MathOperator::Add
                    };
                    value = value.abs();
                }
                (operator, MathNode::literal(value, unit))
            }
            node => (operator, node),
        };
        if let MathNode::Literal { value, unit } = &node {
            if used.contains(unit) {
                continue;
            }
            used.push(*unit);
            // A constant factor of one is left out, unless it is alone.
            if is_multiply && *unit == MathUnit::Number && *value == 1.0 && count > 1 {
                continue;
            }
        }
        result = Some(match result {
            None => {
                // `MaybeNegateFirstNode`.
                match node {
                    MathNode::Literal { value, unit } if operator == MathOperator::Subtract => {
                        MathNode::literal(-value, unit)
                    }
                    node => node.copied(),
                }
            }
            Some(previous) => simplified_arithmetic(previous, node, operator)?,
        });
    }
    result
}

fn maybe_simplify(node: MathNode) -> Option<MathNode> {
    let simplify = (node.is_add_or_subtract() || node.is_multiply()) && !node.nested_intermediate();
    if simplify {
        simplify_sum_or_product(node)
    } else {
        Some(node)
    }
}

// ---- Tokens ----

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Whitespace,
    Number(f64),
    Percentage(f64),
    Dimension(f64, String),
    Ident(String),
    Delim(char),
    /// `(` or a function: the function name, if any, and its contents.
    Block(Option<String>, Vec<Token>),
    /// A token Div does not need, such as `)` outside a block or `,`.
    Other,
}

struct Tokenizer<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> Tokenizer<'a> {
    fn peek(&self, offset: usize) -> Option<u8> {
        self.input.get(self.position + offset).copied()
    }

    fn starts_number(&self) -> bool {
        let digit = |b: Option<u8>| b.is_some_and(|b| b.is_ascii_digit());
        match self.peek(0) {
            Some(b'+' | b'-') => {
                digit(self.peek(1)) || (self.peek(1) == Some(b'.') && digit(self.peek(2)))
            }
            Some(b'.') => digit(self.peek(1)),
            b => digit(b),
        }
    }

    fn is_name(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b >= 0x80
    }

    fn starts_identifier(&self, offset: usize) -> bool {
        let start =
            |b: Option<u8>| b.is_some_and(|b| b.is_ascii_alphabetic() || b == b'_' || b >= 0x80);
        match self.peek(offset) {
            Some(b'-') => start(self.peek(offset + 1)) || self.peek(offset + 1) == Some(b'-'),
            b => start(b),
        }
    }

    fn name(&mut self) -> String {
        let start = self.position;
        while self.peek(0).is_some_and(Self::is_name) {
            self.position += 1;
        }
        String::from_utf8_lossy(&self.input[start..self.position]).into_owned()
    }

    fn number(&mut self) -> Option<f64> {
        let start = self.position;
        if matches!(self.peek(0), Some(b'+' | b'-')) {
            self.position += 1;
        }
        while self.peek(0).is_some_and(|b| b.is_ascii_digit()) {
            self.position += 1;
        }
        if self.peek(0) == Some(b'.') && self.peek(1).is_some_and(|b| b.is_ascii_digit()) {
            self.position += 1;
            while self.peek(0).is_some_and(|b| b.is_ascii_digit()) {
                self.position += 1;
            }
        }
        if matches!(self.peek(0), Some(b'e' | b'E')) {
            let digit_at = |offset: usize| self.peek(offset).is_some_and(|b| b.is_ascii_digit());
            if digit_at(1) {
                self.position += 1;
            } else if matches!(self.peek(1), Some(b'+' | b'-')) && digit_at(2) {
                self.position += 2;
            }
            if self.position > start
                && matches!(self.input[self.position - 1], b'e' | b'E' | b'+' | b'-')
            {
                while self.peek(0).is_some_and(|b| b.is_ascii_digit()) {
                    self.position += 1;
                }
            }
        }
        let text = std::str::from_utf8(&self.input[start..self.position]).ok()?;
        text.parse::<f64>().ok()
    }

    /// Tokens up to the end of the input or, inside a block, its `)`.
    fn tokens(&mut self, in_block: bool) -> Vec<Token> {
        let mut tokens = Vec::new();
        while let Some(b) = self.peek(0) {
            match b {
                b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' => {
                    while matches!(self.peek(0), Some(b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')) {
                        self.position += 1;
                    }
                    tokens.push(Token::Whitespace);
                }
                b')' if in_block => {
                    self.position += 1;
                    return tokens;
                }
                b'(' => {
                    self.position += 1;
                    let contents = self.tokens(true);
                    tokens.push(Token::Block(None, contents));
                }
                _ if self.starts_number() => {
                    let Some(value) = self.number() else {
                        tokens.push(Token::Other);
                        continue;
                    };
                    if self.peek(0) == Some(b'%') {
                        self.position += 1;
                        tokens.push(Token::Percentage(value));
                    } else if self.starts_identifier(0) {
                        let unit = self.name();
                        tokens.push(Token::Dimension(value, unit));
                    } else {
                        tokens.push(Token::Number(value));
                    }
                }
                _ if self.starts_identifier(0) => {
                    let name = self.name();
                    if self.peek(0) == Some(b'(') {
                        self.position += 1;
                        let contents = self.tokens(true);
                        tokens.push(Token::Block(Some(name), contents));
                    } else {
                        tokens.push(Token::Ident(name));
                    }
                }
                b'+' | b'-' | b'*' | b'/' => {
                    self.position += 1;
                    tokens.push(Token::Delim(b as char));
                }
                _ => {
                    self.position += 1;
                    tokens.push(Token::Other);
                }
            }
        }
        tokens
    }
}

// ---- Parser (`CSSMathExpressionNodeParser`) ----

struct Stream<'a> {
    tokens: &'a [Token],
    position: usize,
}

impl<'a> Stream<'a> {
    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.position)
    }

    fn at_end(&self) -> bool {
        self.position >= self.tokens.len()
    }

    fn consume_whitespace(&mut self) {
        while self.peek() == Some(&Token::Whitespace) {
            self.position += 1;
        }
    }

    fn next_is_whitespace(&self) -> bool {
        self.peek() == Some(&Token::Whitespace)
    }
}

fn is_calc(name: &str) -> bool {
    name.eq_ignore_ascii_case("calc") || name.eq_ignore_ascii_case("-webkit-calc")
}

fn parse_value_term(
    stream: &mut Stream,
    depth: u8,
    whitespace_after: &mut bool,
) -> Option<MathNode> {
    match stream.peek()? {
        Token::Block(name, contents) if name.as_deref().is_none_or(is_calc) => {
            stream.position += 1;
            let mut inner = Stream {
                tokens: contents,
                position: 0,
            };
            inner.consume_whitespace();
            let mut result = parse_value_expression(&mut inner, depth)?;
            if !inner.at_end() {
                return None;
            }
            result.set_nested();
            *whitespace_after = stream.next_is_whitespace();
            stream.consume_whitespace();
            Some(result)
        }
        Token::Block(..) => None,
        _ => parse_value(stream, whitespace_after),
    }
}

fn parse_value(stream: &mut Stream, whitespace_after: &mut bool) -> Option<MathNode> {
    let token = stream.peek()?.clone();
    stream.position += 1;
    *whitespace_after = stream.next_is_whitespace();
    stream.consume_whitespace();
    match token {
        Token::Ident(name) => {
            let value = match name.to_ascii_lowercase().as_str() {
                "infinity" => f64::INFINITY,
                "-infinity" => f64::NEG_INFINITY,
                "nan" => f64::NAN,
                "pi" => std::f64::consts::PI,
                "e" => std::f64::consts::E,
                _ => return None,
            };
            Some(MathNode::literal(value, MathUnit::Number))
        }
        Token::Number(value) => Some(MathNode::literal(value, MathUnit::Number)),
        Token::Percentage(value) => Some(MathNode::literal(value, MathUnit::Percent)),
        Token::Dimension(value, unit) => Some(MathNode::literal(value, MathUnit::from_css(&unit)?)),
        _ => None,
    }
}

fn arithmetic_operator(token: Option<&Token>) -> Option<MathOperator> {
    match token? {
        Token::Delim('+') => Some(MathOperator::Add),
        Token::Delim('-') => Some(MathOperator::Subtract),
        Token::Delim('*') => Some(MathOperator::Multiply),
        Token::Delim('/') => Some(MathOperator::Divide),
        _ => None,
    }
}

fn parse_multiplicative(
    stream: &mut Stream,
    depth: u8,
    whitespace_after: &mut bool,
) -> Option<MathNode> {
    let mut result = parse_value_term(stream, depth, whitespace_after)?;
    while !stream.at_end() {
        let operator = match arithmetic_operator(stream.peek()) {
            Some(operator @ (MathOperator::Multiply | MathOperator::Divide)) => operator,
            _ => break,
        };
        stream.position += 1;
        stream.consume_whitespace();
        let right = parse_value_term(stream, depth, whitespace_after)?;
        result = simplified_arithmetic(result, right, operator)?;
    }
    if result.is_multiply() {
        result = maybe_simplify(result)?;
    }
    Some(result)
}

fn parse_additive(stream: &mut Stream, depth: u8) -> Option<MathNode> {
    let mut whitespace_after = false;
    let mut result = parse_multiplicative(stream, depth, &mut whitespace_after)?;
    while !stream.at_end() {
        let operator = match arithmetic_operator(stream.peek()) {
            Some(operator @ (MathOperator::Add | MathOperator::Subtract)) => operator,
            _ => break,
        };
        // `1px+ 2px` and `1px +2px` are invalid.
        if !whitespace_after {
            return None;
        }
        stream.position += 1;
        if !stream.next_is_whitespace() {
            return None;
        }
        stream.consume_whitespace();
        let right = parse_multiplicative(stream, depth, &mut whitespace_after)?;
        result = simplified_arithmetic(result, right, operator)?;
    }
    if result.is_add_or_subtract() {
        result = maybe_simplify(result)?;
    }
    Some(result)
}

fn parse_value_expression(stream: &mut Stream, depth: u8) -> Option<MathNode> {
    let depth = depth + 1;
    if depth > MAX_DEPTH {
        return None;
    }
    parse_additive(stream, depth)
}

/// A parsed `calc()`: Blink's `CSSMathFunctionValue`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "constructor-tokens",
    derive(crate::constructor_tokens::ConstructorTokens)
)]
pub struct MathFunction {
    pub root: MathNode,
}

/// The lengths a math function's relative units resolve against, Blink's
/// `CSSLengthResolver`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MathResolver {
    pub font_size: f32,
    pub root_font_size: f32,
    pub viewport: (f32, f32),
    pub ch: f32,
    pub ex: f32,
    pub lh: f32,
}

impl MathFunction {
    /// Parse `calc(...)` (or `-webkit-calc(...)`), the whole input.
    pub fn parse(input: &str) -> Option<Self> {
        let mut tokenizer = Tokenizer {
            input: input.as_bytes(),
            position: 0,
        };
        let tokens = tokenizer.tokens(false);
        let mut stream = Stream {
            tokens: &tokens,
            position: 0,
        };
        stream.consume_whitespace();
        let Some(Token::Block(Some(name), contents)) = stream.peek() else {
            return None;
        };
        if !is_calc(name) {
            return None;
        }
        stream.position += 1;
        stream.consume_whitespace();
        if !stream.at_end() {
            return None;
        }
        let mut inner = Stream {
            tokens: contents,
            position: 0,
        };
        inner.consume_whitespace();
        let root = parse_value_expression(&mut inner, 0)?;
        if !inner.at_end() {
            return None;
        }
        Some(Self { root })
    }

    pub fn category(&self) -> MathCategory {
        self.root.category()
    }

    /// Whether the value reads the selected font's metrics (`ex`, `ch` or
    /// `lh`).
    pub fn depends_on_font_metrics(&self) -> bool {
        fn visit(node: &MathNode) -> bool {
            match node {
                MathNode::Literal { unit, .. } => {
                    matches!(unit, MathUnit::Ex | MathUnit::Ch | MathUnit::Lh)
                }
                MathNode::Operation(operation) => operation.operands.iter().any(visit),
            }
        }
        visit(&self.root)
    }

    /// `line-height` accepts a number, a length, a percentage or both.
    pub fn is_valid_for_line_height(&self) -> bool {
        self.category() != MathCategory::Intermediate
    }

    /// Blink's `StyleBuilderConverter::ConvertLineHeight` for a math
    /// function, whose range is non-negative.
    pub fn line_height(&self, resolver: &MathResolver) -> Option<LineHeight> {
        let font_size = resolver.font_size;
        match self.category() {
            MathCategory::Number => {
                let value = compute_double(&self.root, resolver).max(0.0);
                let value = if value.is_nan() {
                    0.0
                } else {
                    clamp_double(value)
                };
                // Blink keeps `ClampTo<float>(value * 100)` percent; Div
                // keeps the number, as it does for a number literal.
                Some(LineHeight::Number(value as f32))
            }
            MathCategory::Percent => {
                let value = clamp_double(compute_double(&self.root, resolver).max(0.0));
                let percent = clamp_to_i32(value);
                Some(LineHeight::Length(font_size * percent as f32 / 100.0))
            }
            MathCategory::Length => {
                let value = compute_double(&self.root, resolver).max(0.0);
                Some(LineHeight::Length(clamp_double(value) as f32))
            }
            MathCategory::LengthPercent => {
                let (pixels, percent) = to_pixels_and_percent(&self.root, resolver)?;
                let (pixels, percent) = if (pixels + percent).is_nan() {
                    (0.0, 0.0)
                } else {
                    (clamp_float(pixels), clamp_float(percent))
                };
                let maximum = div_geometry::LayoutUnit::from_f32(font_size).to_f32();
                let value = clamp_float(pixels + percent / 100.0 * maximum);
                let value = if value < 0.0 { 0.0 } else { value };
                Some(LineHeight::Length(
                    div_geometry::LayoutUnit::from_f32(value).to_f32(),
                ))
            }
            MathCategory::Intermediate => None,
        }
    }
}

/// `CSSValueClampingUtils::ClampDouble`: NaN is 0 and infinities are the
/// largest finite doubles.
fn clamp_double(value: f64) -> f64 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(f64::MIN, f64::MAX)
    }
}

/// `CSSValueClampingUtils::ClampLength(float)`.
fn clamp_float(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(f32::MIN, f32::MAX)
    }
}

/// `ClampTo<int>`: saturating, truncating toward zero.
fn clamp_to_i32(value: f64) -> i32 {
    value as i32
}

/// `CSSLengthResolver::ZoomedComputedPixels` at zoom 1.
fn length_px(value: f64, unit: MathUnit, resolver: &MathResolver) -> f64 {
    if let Some(px) = unit.absolute_px() {
        return value * px;
    }
    let (width, height) = (resolver.viewport.0 as f64, resolver.viewport.1 as f64);
    match unit {
        MathUnit::Em => value * resolver.font_size as f64,
        MathUnit::Rem => value * resolver.root_font_size as f64,
        MathUnit::Ex => value * resolver.ex as f64,
        MathUnit::Ch => value * resolver.ch as f64,
        MathUnit::Lh => value * resolver.lh as f64,
        MathUnit::Vw => value * (width / 100.0),
        MathUnit::Vh => value * (height / 100.0),
        MathUnit::Vmin => value * (width.min(height) / 100.0),
        MathUnit::Vmax => value * (width.max(height) / 100.0),
        MathUnit::Number | MathUnit::Percent => value,
        _ => unreachable!("absolute units return above"),
    }
}

/// `ComputeDouble`: a number, a percentage or a length in pixels, through
/// the tree in double precision.
fn compute_double(node: &MathNode, resolver: &MathResolver) -> f64 {
    match node {
        MathNode::Literal { value, unit } => length_px(*value, *unit, resolver),
        MathNode::Operation(operation) => {
            let values: Vec<f64> = operation
                .operands
                .iter()
                .map(|operand| compute_double(operand, resolver))
                .collect();
            evaluate_operator(&values, operation.operator)
        }
    }
}

/// `ToPixelsAndPercent`, in single precision as Blink's `PixelsAndPercent`.
fn to_pixels_and_percent(node: &MathNode, resolver: &MathResolver) -> Option<(f32, f32)> {
    match node {
        MathNode::Literal { value, unit } => Some(match unit.category() {
            MathCategory::Length => (length_px(*value, *unit, resolver) as f32, 0.0),
            MathCategory::Percent => (0.0, *value as f32),
            _ => (clamp_double(*value) as f32, 0.0),
        }),
        MathNode::Operation(operation) => match operation.operator {
            MathOperator::Add | MathOperator::Subtract => {
                let left = to_pixels_and_percent(&operation.operands[0], resolver)?;
                let right = to_pixels_and_percent(&operation.operands[1], resolver)?;
                Some(if operation.operator == MathOperator::Add {
                    (left.0 + right.0, left.1 + right.1)
                } else {
                    (left.0 - right.0, left.1 - right.1)
                })
            }
            MathOperator::Multiply => {
                let (left, right) = (&operation.operands[0], &operation.operands[1]);
                let number_literal =
                    |node: &MathNode| node.is_literal() && node.category() == MathCategory::Number;
                let (number, other) = if number_literal(left) {
                    (left, right)
                } else if number_literal(right) {
                    (right, left)
                } else {
                    return None;
                };
                let result = to_pixels_and_percent(other, resolver)?;
                let number = number.literal_value() as f32;
                Some((result.0 * number, result.1 * number))
            }
            MathOperator::Divide | MathOperator::Invert => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver(font_size: f32) -> MathResolver {
        MathResolver {
            font_size,
            root_font_size: 16.0,
            viewport: (800.0, 600.0),
            ch: font_size / 2.0,
            ex: font_size / 2.0,
            lh: font_size * 1.2,
        }
    }

    fn number(input: &str) -> f64 {
        let function = MathFunction::parse(input).unwrap_or_else(|| panic!("{input}"));
        assert_eq!(function.category(), MathCategory::Number, "{input}");
        compute_double(&function.root, &resolver(16.0))
    }

    #[test]
    fn numbers_simplify_in_double_precision() {
        assert_eq!(number("calc(1.25 / 0.875)"), 1.25 / 0.875);
        assert_eq!(number("calc(1.25 / 0.875)"), 1.4285714285714286);
        assert_eq!(number("calc(1 + 0.5)"), 1.5);
        assert_eq!(number("calc(2 - 0.5 - 0.25)"), 1.25);
        assert_eq!(number("calc(1 - (0.5 - 0.25))"), 0.75);
        assert_eq!(number("calc((1 + 0.25) / 0.875)"), 1.25 / 0.875);
        assert_eq!(number("CALC(1.5)"), 1.5);
        assert_eq!(number("calc( 1.5 )"), 1.5);
        assert_eq!(number("calc(1e0 + 5e-1)"), 1.5);
        assert_eq!(number("calc(calc(1.25) / 0.875)"), 1.25 / 0.875);
        assert_eq!(number("calc(1px / 2px)"), 0.5);
        assert_eq!(number("calc(1 / 0)"), f64::INFINITY);
    }

    #[test]
    fn line_height_takes_the_tailwind_ratio_exactly() {
        let function = MathFunction::parse("calc(1.25 / 0.875)").unwrap();
        assert_eq!(
            function.line_height(&resolver(14.0)),
            Some(LineHeight::Number((1.25f64 / 0.875) as f32))
        );
    }

    #[test]
    fn sums_flatten_and_combine_by_unit() {
        let px = |input: &str, font: f32| {
            let function = MathFunction::parse(input).unwrap_or_else(|| panic!("{input}"));
            assert_eq!(function.category(), MathCategory::Length, "{input}");
            compute_double(&function.root, &resolver(font))
        };
        assert_eq!(px("calc(1em + 2px + 1em)", 14.0), 30.0);
        assert_eq!(px("calc(2px + 1em - 1px)", 14.0), 15.0);
        assert_eq!(px("calc((1em + 2px) * 1.5)", 14.0), 24.0);
        assert_eq!(px("calc(1.5 * (1em + 2px))", 14.0), 24.0);
        assert_eq!(px("calc(1px - (1em - 20px))", 14.0), 7.0);
        assert_eq!(px("calc(1pt + 1px)", 14.0), 96.0 / 72.0 + 1.0);
        assert_eq!(px("calc(1in / 3)", 14.0), 32.0);
        assert_eq!(px("calc(1px * (2))", 14.0), 2.0);
        assert_eq!(px("calc(1px", 14.0), 1.0);
    }

    #[test]
    fn invalid_expressions_are_rejected() {
        for input in [
            "calc(1 + 1px)",
            "calc(1px + 1)",
            "calc(1% + 1)",
            "calc(1px+2px)",
            "calc(1px +2px)",
            "calc(1px -2px)",
            "calc()",
            "calc(1px))",
            "calc(1px 2px)",
            "calc(1deg)",
            "calc(1px) 2px",
            "min(1px, 2px)",
            "1px",
        ] {
            assert!(MathFunction::parse(input).is_none(), "{input}");
        }
        for input in ["calc(1px * 2px)", "calc(2 / 1px)"] {
            assert_eq!(
                MathFunction::parse(input).map(|f| f.category()),
                Some(MathCategory::Intermediate),
                "{input}"
            );
        }
    }

    #[test]
    fn length_and_percent_fold_in_single_precision() {
        let function = MathFunction::parse("calc(110% - 3px + 0.25em)").unwrap();
        assert_eq!(function.category(), MathCategory::LengthPercent);
        assert_eq!(
            to_pixels_and_percent(&function.root, &resolver(14.0)),
            Some((-3.0 + 3.5, 110.0))
        );
    }
}
