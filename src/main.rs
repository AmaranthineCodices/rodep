extern crate clap;
use clap::{App, Arg, SubCommand};

extern crate git2;
use git2::Repository;

extern crate url;
use url::Url;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

extern crate serde;
extern crate serde_json;

mod config;
use config::Config;

use std::path::{PathBuf, Path};
use std::io::prelude::*;
use std::fs::File;

use serde_json::Value;

lazy_static! {
    static ref GH_BASE_URL: Url = Url::parse("https://github.com/").unwrap();
}

fn cloned_name(url: &Url) -> &str {
    url.path_segments().expect("no path segments").last().expect("no last value")
}

// FIXME: Result should also handle other errors, not just io::Error.
fn add_submodule_to_rojo(cfg: &Config, submodule_name: &str, submodule_src_dir: &Path, target_name: &str) -> Result<(), std::io::Error> {
    let mut file = File::open(cfg.rojo_path)?;

    let mut file_contents = String::new();
    file.read_to_string(&mut file_contents)?;
    
    // Manipulate the Rojo JSON file dynamically, without static typing
    // This allows the configuration to be *mostly* independent of Rojo's
    // configuration format.
    let mut json_tree: Value = serde_json::from_str(&file_contents).expect("can't parse json");
    let mut partition_map = serde_json::Map::new();

    let mut src_path = PathBuf::new();
    src_path.push(cfg.lib_dir);
    src_path.push(submodule_name);
    src_path.push(submodule_src_dir);
    // TODO: Find src/lib path - this is complicated
    let src_path_str = src_path.to_str().expect("can't convert path to string").to_owned();
    partition_map.insert("path".to_owned(), Value::String(src_path_str));
    partition_map.insert("target".to_owned(), Value::String(format!("{}.{}", cfg.lib_target, target_name)));
    // This partition key is potentially a problem
    json_tree["partitions"][format!("__rodep_auto_{}", submodule_name)] = Value::Object(partition_map);

    // Pretty-print the Rojo configuration, as it'll be edited by users
    let altered_rojo_cfg = serde_json::to_string_pretty(&json_tree).unwrap();

    // Does this leak file handles, since the file handle from reading is being
    // masked, or is Rust smart enough to drop the old handle?
    let mut file = File::create(cfg.rojo_path)?;
    file.write_all(altered_rojo_cfg.as_bytes())?;

    Ok(())
}

fn get_src_directory(repository_path: &Path) -> Result<PathBuf, ()> {
    // Try in this order:
    // src
    // lib
    // src will be present for some, and the lib folder will be used for
    // storing that repository's submodules.
    // in others, lib will be the source folder, and modules will be the
    // submodules root.

    let src_path = repository_path.join("src");
    let lib_path = repository_path.join("lib");

    if src_path.is_dir() {
        Ok(src_path)
    }
    else if lib_path.is_dir() {
        Ok(lib_path)
    }
    else {
        Err(())
    }
}

fn main() {
    let matches = App::new("rodep")
        .version("0.1.0")
        .author("AmaranthineCodices")
        .about("Super simple dependency adder")
        .arg(Arg::with_name("config")
            .short("c")
            .long("cfg")
            .takes_value(true)
            .default_value("rodep.json")
            .help("the rodep configuration file. Use rodep init to generate a new one."))
        .subcommand(SubCommand::with_name("init")
            .about("Creates a starter configuration file in this directory."))
        .subcommand(SubCommand::with_name("add")
                .about("Adds dependencies.")
                .arg(Arg::with_name("name")
                    // Allow hyphens in repository names - they happen!
                    .allow_hyphen_values(true)
                    .required(true)
                    .takes_value(true)
                    .index(1)
                    .help("the repository name to clone"))
                .arg(Arg::with_name("dir")
                    .short("d")
                    .long("dir")
                    .takes_value(true)
                    .help("the source directory of the repository. If unspecified, will use src or lib, whichever exists.")
                )
                .arg(Arg::with_name("target")
                    .short("t")
                    .long("target-name")
                    .takes_value(true)
                    .help("the name to use when synchronizing into Roblox Studio")
                )
        )
        .get_matches();

    // Match init first, so we can stop if that's the subcommand.
    // All other commands require a configuration; init creates one.
    if let Some(_) = matches.subcommand_matches("init") {
        let default_cfg = Config {
            lib_target: "ReplicatedStorage",
            lib_dir: "lib",
            rojo_path: "rojo.json",
        };

        // This should always serialize successfully.
        let serialized = serde_json::to_string(&default_cfg).unwrap();

        // Use expect - if the error cannot be handled we're SOL; the user
        // needs to intervene here
        let mut file = File::create(&Path::new("rodep.json")).expect("couldn't create rodep.json");
        file.write_all(serialized.as_bytes()).expect("couldn't write to rodep.json");

        // Tell the user we made a file! It's disconcerting if the command just
        // runs with no output.
        println!("created rodep.json configuration file");

        // Halt execution, we're done!
        return
    }

    // Load configuration file now
    // The only subcommand that can act without a configuration is init, which
    // creates a cfg file. Once it's done, we can load it.
    // The config argument will always have a value, since a default is specified.
    let config_path_str = matches.value_of("config").unwrap();
    let config_path = Path::new(config_path_str);
    let mut file = File::open(&config_path).expect("could not open config file");

    let mut file_contents = String::new();
    file.read_to_string(&mut file_contents).expect("could not read config file");
    let config: Config = serde_json::from_str(&file_contents).unwrap();

    if let Some(matches) = matches.subcommand_matches("add") {
        let cwd = std::env::current_dir().unwrap();
        let repository = Repository::discover(cwd.as_path()).expect("couldn't find repository");

        if let Some(name) = matches.value_of("name") {
            if let Ok(repo_url) = GH_BASE_URL.join(name) {
                let clone_name = cloned_name(&repo_url);
                let repo_url_str = repo_url.as_str().to_owned();

                let mut path = PathBuf::new();
                path.push(&config.lib_dir);
                path.push(clone_name);

                let mut submodule = repository.submodule(&repo_url_str, path.as_path(), false).expect("couldn't create submodule");
                // Immediately open the submodule; we can't really do
                // anything with the Submodule struct itself.
                let submodule_repository = submodule.open().expect("couldn't open submodule repo");

                // Find the origin and fetch the master branch from it.
                // Since this is GitHub, we can assume it has a master
                // branch; if not, we probably need to panic.
                submodule_repository.find_remote("origin").unwrap().fetch(&["master"], None, None).expect("couldn't fetch master");
                // Find the latest commit to master and peel it to the commit.
                let origin_master_obj = submodule_repository.revparse_single("origin/master").expect("couldn't find ref to origin/master");
                let commit = origin_master_obj.peel_to_commit().expect("couldn't get commit");
                // Create a local branch based on the contents of the master.
                submodule_repository.branch("master", &commit, true).expect("couldn't create local master branch");

                let mut cb = git2::build::CheckoutBuilder::new();
                // MUST checkout with force, otherwise the Git repository
                // thinks we just deleted everything x.x
                cb.force();
                
                // Check out files into the working directory. Everything
                // up until this point has been prep-work.
                submodule_repository.checkout_tree(&origin_master_obj, Some(&mut cb)).expect("couldn't checkout master");
                // Set the repository HEAD to the local master branch.
                submodule_repository.set_head("refs/heads/master").expect("couldn't set head");
                // Finalize the submodule addition - adds it to .gitmodules
                // and the like.
                submodule.add_finalize().expect("couldn't finalize submodule");

                let src_dir = get_src_directory(&path).unwrap_or_else(
                    |_| PathBuf::from(matches.value_of("dir").expect("couldn't infer a source directory; please specify one with --dir")));

                let sync_name = matches.value_of("target").unwrap_or(clone_name);

                // Add the submodule to the Rojo configuration!
                add_submodule_to_rojo(&config, clone_name, &src_dir, &sync_name).expect("couldn't add submodule to rojo config");
                println!("added submodule {} in {}", clone_name, path.as_path().to_str().unwrap());
            }
            else {
                println!("Invalid repository name {}", name);
            }
        }
    }
}
