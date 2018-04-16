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
use std::collections::HashMap;

use serde_json::Value;

fn cloned_name(url: &Url) -> &str {
    url.path_segments().expect("no path segments").last().expect("no last value")
}

lazy_static! {
    static ref GH_BASE_URL: Url = Url::parse("https://github.com/").unwrap();
}

fn add_submodule_to_rojo(cfg: &Config, submodule_name: &str) -> Result<(), std::io::Error> {
    let mut file = File::open(cfg.rojo_path)?;

    let mut file_contents = String::new();
    file.read_to_string(&mut file_contents)?;
    
    let mut json_tree: Value = serde_json::from_str(&file_contents).expect("can't parse json");
    let mut partition_map = serde_json::Map::new();
    // TODO: Use Path to join
    partition_map.insert("src".to_owned(), Value::String(format!("{}/{}", cfg.lib_dir, submodule_name)));
    partition_map.insert("target".to_owned(), Value::String(format!("{}.{}", cfg.lib_target, submodule_name)));
    json_tree["partitions"][format!("__rodep_auto_{}", submodule_name)] = Value::Object(partition_map);

    let altered_rojo_cfg = serde_json::to_string_pretty(&json_tree).unwrap();

    let mut file = File::create(cfg.rojo_path)?;
    file.write_all(altered_rojo_cfg.as_bytes())?;

    Ok(())
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
                    .multiple(true)
                    .allow_hyphen_values(true)
                    .required(true)
                    .help("the repository name(s) to clone"))
        )
        .get_matches();

    if let Some(_) = matches.subcommand_matches("init") {
        let default_cfg = Config {
            lib_target: "ReplicatedStorage",
            lib_dir: "lib",
            rojo_path: "rojo.json",
        };

        let serialized = serde_json::to_string(&default_cfg).unwrap();
        let mut file = match File::create(&Path::new("rodep.json")) {
            Ok(file) => file,
            Err(why) => panic!("couldn't create file rodep.json: {}", why),
        };

        match file.write_all(serialized.as_bytes()) {
            Err(why) => panic!("couldn't write to rodep.json: {}", why),
            _ => println!("created rodep.json configuration file"),
        };

        return;
    }

    // Load configuration file now
    // The only subcommand that can act without a configuration is init, which
    // creates a cfg file. Once it's done, we can load it.
    let config_path_str = matches.value_of("config").unwrap();
    let config_path = Path::new(config_path_str);
    let mut file = File::open(&config_path).expect("could not open config file");

    let mut file_contents = String::new();
    file.read_to_string(&mut file_contents).expect("could not read config file");
    let config: Config = serde_json::from_str(&file_contents).unwrap();

    if let Some(matches) = matches.subcommand_matches("add") {
        let cwd = std::env::current_dir().unwrap();
        let repository = Repository::discover(cwd.as_path()).expect("couldn't find repository");

        if let Some(names) = matches.values_of("name") {
            for name in names {
                if let Ok(repo_url) = GH_BASE_URL.join(name) {
                    let clone_name = cloned_name(&repo_url);
                    let repo_url_str = repo_url.as_str().to_owned();

                    let mut path = PathBuf::new();
                    {
                        path.push(&config.lib_dir);
                        path.push(clone_name);
                    }

                    let mut submodule = repository.submodule(&repo_url_str, path.as_path(), false).expect("couldn't create submodule");

                    let submodule_repository = submodule.open().expect("couldn't open submodule repo");
                    
                    submodule_repository.find_remote("origin").unwrap().fetch(&["master"], None, None).expect("couldn't fetch master");
                    let origin_master_obj = submodule_repository.revparse_single("origin/master").expect("couldn't find ref to origin/master");
                    let commit = origin_master_obj.peel_to_commit().expect("couldn't get commit");
                    submodule_repository.branch("master", &commit, true).expect("couldn't create local master branch");

                    let mut cb = git2::build::CheckoutBuilder::new();
                    cb.force();
                    
                    submodule_repository.checkout_tree(&commit.as_object(), Some(&mut cb)).expect("couldn't checkout master");
                    submodule_repository.set_head("refs/heads/master").expect("couldn't set head");
                    submodule.add_finalize().expect("couldn't finalize submodule");

                    add_submodule_to_rojo(&config, clone_name).expect("couldn't add submodule to rojo config");
                    println!("added submodule {} in {}", clone_name, path.as_path().to_str().unwrap());
                }
                else {
                    println!("Invalid repository name {}", name);
                }
            }
        }
    }
}
