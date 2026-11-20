//! The number, flag and separator grammar shared by SVG's micro-syntaxes
//! (SVG 2 path data grammar, `points`, `viewBox`, transform lists).

pub(crate) struct Scanner<'a> {
    bytes: &'a [u8],
    pub(crate) pos: usize,
}

fn is_ws(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

/// The value of a number token, accumulated in single precision as
/// Chromium's attribute parser does, which is not the correctly rounded
/// value: the corpus's `path/exponents` reads `1.9e2` as 190.00001525878906.
/// The integer digits accumulate as `value × 10 + digit`. The fraction
/// digits accumulate on their own, each adding `digit × scale` with the
/// scale shrinking by `0.1` per digit, and the fraction is then added to
/// the integer part. The exponent then multiplies by `10^exponent`.
///
/// The order matters. `path/fractions` reads `1.354` as 1.3540000915527344,
/// where adding each fraction digit to the value gives 1.3539998531341553
/// and correct rounding 1.3539999723434448. The same order reads `5.33` as
/// the float nearest 5.33, not one ulp above, which centres the arcs of
/// the paint corpus's `chart/legend` exactly.
fn accumulate(token: &[u8]) -> f32 {
    let mut bytes = token.iter().copied().peekable();
    let negative = match bytes.peek() {
        Some(b'-') => {
            bytes.next();
            true
        }
        Some(b'+') => {
            bytes.next();
            false
        }
        _ => false,
    };
    let mut value = 0.0f32;
    while let Some(digit) = bytes.next_if(u8::is_ascii_digit) {
        value = value * 10.0 + f32::from(digit - b'0');
    }
    if bytes.next_if_eq(&b'.').is_some() {
        let mut scale = 1.0f32;
        let mut fraction = 0.0f32;
        // Added to the integer part only once it is complete.
        while let Some(digit) = bytes.next_if(u8::is_ascii_digit) {
            scale *= 0.1;
            fraction += f32::from(digit - b'0') * scale;
        }
        value += fraction;
    }
    if bytes.next().is_some() {
        let mut exponent_negative = false;
        match bytes.peek() {
            Some(b'-') => {
                bytes.next();
                exponent_negative = true;
            }
            Some(b'+') => {
                bytes.next();
            }
            _ => {}
        }
        let mut exponent = 0i32;
        for digit in bytes {
            exponent = exponent
                .saturating_mul(10)
                .saturating_add(i32::from(digit - b'0'));
        }
        if exponent_negative {
            exponent = -exponent;
        }
        value *= 10.0f32.powi(exponent);
    }
    if negative {
        -value
    } else {
        value
    }
}

impl<'a> Scanner<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            pos: 0,
        }
    }

    pub(crate) fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    pub(crate) fn skip_ws(&mut self) {
        while self.peek().is_some_and(is_ws) {
            self.pos += 1;
        }
    }

    /// `wsp* ","? wsp*`. Returns whether a comma was consumed.
    pub(crate) fn skip_comma_ws(&mut self) -> bool {
        self.skip_ws();
        let comma = self.peek() == Some(b',');
        if comma {
            self.pos += 1;
            self.skip_ws();
        }
        comma
    }

    /// Consumes `literal` if it comes next.
    pub(crate) fn eat(&mut self, literal: &str) -> bool {
        if self.bytes[self.pos..].starts_with(literal.as_bytes()) {
            self.pos += literal.len();
            true
        } else {
            false
        }
    }

    /// Whether a number can start at the cursor.
    pub(crate) fn starts_number(&self) -> bool {
        matches!(self.peek(), Some(b'0'..=b'9' | b'.' | b'+' | b'-'))
    }

    /// `sign? (digits ("." digits?)? | "." digits) exponent?`, where the
    /// exponent is taken only when digits follow it. The cursor is left
    /// unchanged on failure.
    pub(crate) fn number(&mut self) -> Option<f32> {
        let start = self.pos;
        let mut pos = self.pos;
        let at = |pos: usize| self.bytes.get(pos).copied();
        if matches!(at(pos), Some(b'+' | b'-')) {
            pos += 1;
        }
        let int_start = pos;
        while at(pos).is_some_and(|b| b.is_ascii_digit()) {
            pos += 1;
        }
        let mut digits = pos - int_start;
        if at(pos) == Some(b'.') {
            let frac_start = pos + 1;
            let mut end = frac_start;
            while at(end).is_some_and(|b| b.is_ascii_digit()) {
                end += 1;
            }
            if digits > 0 || end > frac_start {
                digits += end - frac_start;
                pos = end;
            }
        }
        if digits == 0 {
            return None;
        }
        if matches!(at(pos), Some(b'e' | b'E')) {
            let mut end = pos + 1;
            if matches!(at(end), Some(b'+' | b'-')) {
                end += 1;
            }
            let exp_start = end;
            while at(end).is_some_and(|b| b.is_ascii_digit()) {
                end += 1;
            }
            if end > exp_start {
                pos = end;
            }
        }
        let value = accumulate(&self.bytes[start..pos]);
        if !value.is_finite() {
            return None;
        }
        self.pos = pos;
        Some(value)
    }

    /// A path-data flag: exactly `0` or `1`, with no separator required
    /// after it.
    pub(crate) fn flag(&mut self) -> Option<bool> {
        let value = match self.peek()? {
            b'0' => false,
            b'1' => true,
            _ => return None,
        };
        self.pos += 1;
        Some(value)
    }

    /// A number followed by an optional comma-whitespace separator.
    pub(crate) fn number_sep(&mut self) -> Option<f32> {
        let value = self.number()?;
        self.skip_comma_ws();
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::Scanner;

    fn numbers(text: &str) -> Vec<f32> {
        let mut scanner = Scanner::new(text);
        let mut out = Vec::new();
        scanner.skip_ws();
        while let Some(value) = scanner.number_sep() {
            out.push(value);
        }
        out
    }

    #[test]
    fn splits_numbers_without_separators() {
        assert_eq!(numbers("10-10"), [10.0, -10.0]);
        assert_eq!(numbers("190.5.5"), [190.5, 0.5]);
        assert_eq!(numbers("-1e1 1E2"), [-10.0, 100.0]);
        assert_eq!(numbers("1e+1,.75"), [10.0, 0.75]);
    }

    #[test]
    fn accumulates_the_fraction_on_its_own() {
        // Chromium 147's readings (geometry corpus `path/fractions`).
        assert_eq!(
            numbers("1.354 1.771 1.834"),
            [1.354_000_1, 1.770_999_9, 1.834_000_1]
        );
        assert_eq!(f64::from(numbers("1.354")[0]), 1.3540000915527344);
        assert_eq!(numbers("5.33"), [5.33]);
        assert_eq!(numbers("1.9e2"), [190.000_02]);
    }

    #[test]
    fn exponent_needs_digits() {
        let mut scanner = Scanner::new("2em");
        assert_eq!(scanner.number(), Some(2.0));
        assert_eq!(scanner.peek(), Some(b'e'));
    }

    #[test]
    fn rejects_bare_signs_and_dots() {
        assert_eq!(numbers("-"), Vec::<f32>::new());
        assert_eq!(numbers("."), Vec::<f32>::new());
    }
}
