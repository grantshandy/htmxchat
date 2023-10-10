use std::{process::Command, env};

fn main() {
	let tailwind_cmd = format!("tailwindcss --minify -i src/input.css -o {}/output.css", env::var("OUT_DIR").unwrap());

    println!("{tailwind_cmd}");

	if cfg!(target_os = "windows") {
		Command::new("cmd").arg("/C").arg(tailwind_cmd).status()
	} else {
		Command::new("sh").arg("-c").arg(tailwind_cmd).status()
	}
	.expect("error running tailwind");

	println!("cargo:rerun-if-changed=<FILE>");
}
