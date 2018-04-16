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

mod cfg;

use std::path::{PathBuf, Path};
use std::io::prelude::*;
use std::fs::File;

fn cloned_name(url: &Url) -> &str {
    url.path_segments().expect("no path segments").last().expect("no last value")
}

lazy_static! {
    static ref GH_BASE_URL: Url = Url::parse("https://github.com/").unwrap();
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
                .arg(Arg::with_name("dir")
                    .short("d")
                    .long("dir")
                    .required(true)
                    .takes_value(true)
                    .help("the directory to clone the submodules into"))
        )
        .get_matches();


    if let Some(matches) = matches.subcommand_matches("add") {
        let cwd = std::env::current_dir().unwrap();
        let repository = Repository::discover(cwd.as_path()).expect("couldn't find repository");
        let lib_dir = matches.value_of("dir").expect("no dir");

        if let Some(names) = matches.values_of("name") {
            for name in names {
                if let Ok(repo_url) = GH_BASE_URL.join(name) {
                    let mut path = PathBuf::new();
                    {
                        let clone_name = cloned_name(&repo_url);
                        path.push(lib_dir);
                        path.push(clone_name);
                    }

                    let repo_url_str = repo_url.as_str().to_owned();

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
                }
                else {
                    println!("Invalid repository name {}", name);
                }
            }
        }
    }
    else if let Some(_) = matches.subcommand_matches("init") {
        let default_cfg = cfg::Cfg {
            lib_target: "ReplicatedStorage".to_owned(),
            lib_dir: "lib".to_owned()
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
    }
}
