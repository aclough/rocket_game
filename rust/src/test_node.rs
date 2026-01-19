use godot::prelude::*;

#[derive(GodotClass)]
#[class(base=Node)]
pub struct TestNode {
    base: Base<Node>,
}

#[godot_api]
impl INode for TestNode {
    fn init(base: Base<Node>) -> Self {
        godot_print!("TestNode initialized - Rust is connected!");
        Self { base }
    }

    fn ready(&mut self) {
        godot_print!("TestNode ready - Godot-Rust integration working!");
    }
}

#[godot_api]
impl TestNode {
    #[func]
    pub fn test_connection(&self) -> GString {
        GString::from("Rust connection successful!")
    }
}
