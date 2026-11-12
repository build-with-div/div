//! Tree order of elements, as Radix's `Collection`, which reads a group's
//! items in DOM order.

use div::prelude::Element;

/// The child indices from the root down to `element`. Comparing two paths
/// compares the elements' positions in tree order.
fn path(element: &Element) -> Vec<usize> {
    let mut path = Vec::new();
    let mut current = element.clone();
    while let Some(parent) = current.parent().expect("the document answers") {
        let mut index = 0;
        let mut sibling = parent.first_child().expect("the document answers");
        while let Some(child) = sibling {
            if child.is_same_node(&current).expect("the document answers") {
                break;
            }
            index += 1;
            sibling = child.next_sibling().expect("the document answers");
        }
        path.push(index);
        current = parent;
    }
    path.reverse();
    path
}

/// Sort `items` into tree order by the element each holds.
pub fn sort_in_tree_order<T>(items: &mut [T], element: impl Fn(&T) -> Element) {
    items.sort_by_cached_key(|item| path(&element(item)));
}
