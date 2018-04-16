#[derive(Serialize, Deserialize)]
pub struct Config<'a> {
	pub lib_target: &'a str,
	pub lib_dir: &'a str,
	pub rojo_path: &'a str,
}