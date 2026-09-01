//! An IconId cannot be fabricated without the atlas owner that validates its slot.

use microui_redux::IconId;

fn main() {
    let _icon = IconId::default();
}
