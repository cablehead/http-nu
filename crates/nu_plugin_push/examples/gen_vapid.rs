// Small standalone keygen for development / CI -- prints a VAPID keypair as
// `export VAR=value` lines you can eval. The plugin command
// `push vapid generate` is the canonical interface from nushell; this binary
// just gives a CLI shortcut.
//
//   cargo run --example gen_vapid -p nu_plugin_push
//   eval "$(cargo run --example gen_vapid -p nu_plugin_push 2>/dev/null)"

fn main() {
    let kp = nu_plugin_push::vapid::generate().expect("vapid generate");
    println!("export VAPID_PUBLIC_KEY={}", kp.public_key_b64url);
    println!("export VAPID_PRIVATE_KEY={}", kp.private_key_b64url);
    eprintln!();
    eprintln!("# Public key for the browser (applicationServerKey):");
    eprintln!("#   {}", kp.public_key_b64url);
}
