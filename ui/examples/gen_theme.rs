//! Writes `src/theme.rs`: `cargo run --example gen_theme > src/theme.rs` (in ui/).
fn main() {
    print!("{}", seasoncat_ui::colors::theme_source());
}
