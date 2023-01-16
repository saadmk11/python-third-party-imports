use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use clap::Parser;
use jwalk::{Parallelism, WalkDir};
use rustpython_ast::{AliasData, Located};
use rustpython_parser::ast::StmtKind;
use rustpython_parser::parser;

use builtins::STANDARD_LIBRARY;

mod builtins;

#[derive(Debug, Parser)]
#[command(
    author,
    about = "Find Third Party Package Imports in Your Python Project."
)]
#[command(version)]
pub struct Arguments {
    /// Path to the Project Root Directory.
    #[arg(value_parser = parse_project_root)]
    pub project_root: PathBuf,
}

fn parse_project_root(arg: &str) -> Result<PathBuf, String> {
    let path_buf = PathBuf::from(arg);

    if !path_buf.exists() {
        Err("Path does not exist".to_string())
    } else if !path_buf.is_dir() {
        Err("Path to a directory is required".to_string())
    } else {
        Ok(path_buf)
    }
}

pub fn main() {
    let now = Instant::now();

    let args = Arguments::parse();

    let mut third_party_packages: HashSet<String> = HashSet::new();
    let mut handles: Vec<JoinHandle<Option<HashSet<String>>>> = Vec::new();

    let project_root = Arc::new(args.project_root);

    for entry in WalkDir::new(&*project_root)
        .parallelism(Parallelism::RayonNewPool(0))
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "py" || ext == "pyi")
                .unwrap_or_default()
        })
    {
        let root = Arc::clone(&project_root);

        let handle = thread::spawn(move || -> Option<HashSet<String>> {
            let file_path = entry.path();
            let content = fs::read_to_string(&file_path).ok()?;

            if let Ok(python_ast) = parser::parse_program(&content, &file_path.to_string_lossy()) {
                return Some(find_third_party_packages(&root, &file_path, &python_ast));
            }
            None
        });
        handles.push(handle);
    }

    for handle in handles {
        if let Ok(Some(packages)) = handle.join() {
            third_party_packages.extend(packages);
        }
    }

    println!("THIRD PARTY PACKAGES: {:#?}", third_party_packages);
    println!("LENGTH: {}", third_party_packages.len());
    println!("ELAPSED: {:.2?}", now.elapsed());
}

fn find_third_party_packages<'a>(
    project_root: &PathBuf,
    file_path: &PathBuf,
    python_ast: &Vec<Located<StmtKind>>,
) -> HashSet<String> {
    let mut third_party_packages: HashSet<String> = HashSet::new();

    for ast in python_ast {
        match &ast.node {
            StmtKind::Import { names } => {
                for name in names {
                    let AliasData { name: module, .. } = &name.node;

                    if let Some(module_base) =
                        is_third_party_package(project_root, file_path, module)
                    {
                        third_party_packages.insert(module_base.to_string());
                    }
                }
            }
            StmtKind::ImportFrom {
                module,
                names: _names,
                level,
            } => {
                if let Some(l) = level {
                    if *l == 0 {
                        if let Some(module) = module {
                            if let Some(module_base) =
                                is_third_party_package(project_root, file_path, module)
                            {
                                third_party_packages.insert(module_base.to_string());
                            }
                        }
                    }
                }
            }
            _ => (),
        }
    }
    third_party_packages
}

fn is_local_import(project_root: &PathBuf, file_path: &PathBuf, module: &str) -> bool {
    if let Ok(project_root_canonical) = project_root.canonicalize() {
        if project_root_canonical.ends_with(module) {
            return true;
        }
    }
    if project_root.join(module).is_dir() || project_root.join(format!("{module}.py")).is_file() {
        return true;
    }
    if let Some(parent) = file_path.parent() {
        if parent.join(module).is_dir() || parent.join(format!("{module}.py")).is_file() {
            return true;
        }
    }
    false
}

fn is_third_party_package<'a>(
    project_root: &PathBuf,
    file_path: &PathBuf,
    module: &'a str,
) -> Option<&'a str> {
    let module_base = match module.split_once(".") {
        Some((first, _)) => first,
        None => module,
    };

    if !STANDARD_LIBRARY.contains(&module_base)
        && !is_local_import(project_root, file_path, module_base)
    {
        Some(module_base)
    } else {
        None
    }
}
