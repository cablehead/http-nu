use log::trace;
use nu_protocol::{
    engine::{EngineState, StateWorkingSet, VirtualPath},
    VirtualPathId,
};
use std::path::PathBuf;

// Embedded stdlib files
const STDLIB_MOD: &str = include_str!("mod.nu");
const ROUTER_MOD: &str = include_str!("router/mod.nu");
const HTML_MOD: &str = include_str!("html/mod.nu");
const DATASTAR_MOD: &str = include_str!("datastar/mod.nu");

fn create_virt_file(working_set: &mut StateWorkingSet, name: &str, content: &str) -> VirtualPathId {
    let sanitized_name = PathBuf::from(name).to_string_lossy().to_string();
    let file_id = working_set.add_file(sanitized_name.clone(), content.as_bytes());
    working_set.add_virtual_path(sanitized_name, VirtualPath::File(file_id))
}

/// Load the http-nu standard library into the engine state
///
/// This embeds the stdlib modules at compile time and makes them available
/// via the virtual filesystem. Users can import with: use http-nu/router *, use http-nu/html *
pub fn load_http_nu_stdlib(engine_state: &mut EngineState) -> Result<(), miette::ErrReport> {
    trace!("load_http_nu_stdlib");

    let mut working_set = StateWorkingSet::new(engine_state);
    let mut http_nu_virt_paths = vec![];

    // http-nu/mod.nu (main entry point)
    let std_mod_virt_file_id = create_virt_file(&mut working_set, "http-nu/mod.nu", STDLIB_MOD);
    http_nu_virt_paths.push(std_mod_virt_file_id);

    // Submodules
    let std_submodules = vec![
        ("mod.nu", "http-nu/router", ROUTER_MOD),
        ("mod.nu", "http-nu/html", HTML_MOD),
        ("mod.nu", "http-nu/datastar", DATASTAR_MOD),
    ];

    for (filename, std_subdir_name, content) in std_submodules {
        let mod_dir = PathBuf::from(std_subdir_name);
        let name = mod_dir.join(filename);
        let virt_file_id = create_virt_file(&mut working_set, &name.to_string_lossy(), content);

        let mod_dir_filelist = vec![virt_file_id];
        let virt_dir_id = working_set.add_virtual_path(
            mod_dir.to_string_lossy().to_string(),
            VirtualPath::Dir(mod_dir_filelist),
        );
        http_nu_virt_paths.push(virt_dir_id);
    }

    // Create http-nu virtual directory
    let std_dir = PathBuf::from("http-nu").to_string_lossy().to_string();
    let _ = working_set.add_virtual_path(std_dir, VirtualPath::Dir(http_nu_virt_paths));

    engine_state.merge_delta(working_set.render())?;

    Ok(())
}
