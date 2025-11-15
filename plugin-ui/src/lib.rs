wit_bindgen::generate!({
    world: "plugin",
    generate_all
});

use exports::test::Guest as TestGuest;

struct Component;

impl TestGuest for Component {
    fn get_number() -> u32 {
        42
    }
}

export!(Component);