use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use clap::Parser;
use jwalk::{Parallelism, WalkDir};
use rustpython_ast::{AliasData, ExcepthandlerKind, Located};
use rustpython_parser::ast::StmtKind;
use rustpython_parser::parser;

use builtins::STANDARD_LIBRARY;

mod builtins;

#[derive(Debug, Parser)]
#[command(
    author,
    about = "Find all third-party packages imported into your python project."
)]
#[command(version)]
pub struct Arguments {
    /// Path to the project's root directory.
    #[arg(value_parser = project_root_value_parser)]
    pub project_root: PathBuf,
}

fn project_root_value_parser(arg: &str) -> Result<PathBuf, String> {
    let path_buf = PathBuf::from(arg);

    if !path_buf.exists() {
        Err("Path does not exist".to_string())
    } else if !path_buf.is_dir() {
        Err("Path must be a directory".to_string())
    } else {
        Ok(path_buf)
    }
}

pub fn main() {
    let now = Instant::now();
    let args = Arguments::parse();
    let (file_count, third_party_packages): (usize, HashSet<String>) = run(args.project_root);

    println!(
        "Found '{}' third-party package imports in '{}' files. (Took {:.2?})\n",
        third_party_packages.len(),
        file_count,
        now.elapsed()
    );
    third_party_packages.iter().for_each(|package| {
        println!("{package}");
    });
}

/// Traverse all the python files in project root and return
/// all third-party package imported and the number of files parsed
fn run(project_root: PathBuf) -> (usize, HashSet<String>) {
    let mut third_party_packages: HashSet<String> = HashSet::new();
    let mut handles: Vec<JoinHandle<Option<HashSet<String>>>> = Vec::new();

    let project_root = Arc::new(project_root);

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
        let root_path = Arc::clone(&project_root);

        let handle = thread::spawn(move || -> Option<HashSet<String>> {
            let file_path = entry.path();
            let content = fs::read_to_string(&file_path).ok()?;

            if let Ok(python_ast) = parser::parse_program(&content, &file_path.to_string_lossy()) {
                Some(find_third_party_packages(
                    &root_path,
                    &file_path,
                    &python_ast,
                ))
            } else {
                None
            }
        });
        handles.push(handle);
    }

    let file_count: usize = handles.len();

    for handle in handles {
        if let Ok(Some(packages)) = handle.join() {
            third_party_packages.extend(packages);
        }
    }
    (file_count, third_party_packages)
}

/// Find third-party imports in a single python file
fn find_third_party_packages(
    project_root: &PathBuf,
    file_path: &PathBuf,
    python_ast: &Vec<Located<StmtKind>>,
) -> HashSet<String> {
    let mut third_party_packages: HashSet<String> = HashSet::new();

    for stmt in python_ast {
        parse_stmt(stmt, project_root, file_path, &mut third_party_packages);
    }
    third_party_packages
}

/// Recursively parse python ast and update `third_party_packages`
/// if third-party import is found
fn parse_stmt(
    stmt: &Located<StmtKind>,
    project_root: &PathBuf,
    file_path: &PathBuf,
    third_party_packages: &mut HashSet<String>,
) {
    match &stmt.node {
        StmtKind::Import { names } => {
            for name in names {
                let AliasData { name: module, .. } = &name.node;

                if let Some(module_base) = is_third_party_package(project_root, file_path, module) {
                    third_party_packages.insert(module_base.to_string());
                }
            }
        }
        StmtKind::ImportFrom {
            module,
            names: _names,
            level,
        } => {
            if let Some(level_val) = level {
                if *level_val == 0 {
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
        StmtKind::AsyncFunctionDef { body, .. }
        | StmtKind::FunctionDef { body, .. }
        | StmtKind::ClassDef { body, .. }
        | StmtKind::AsyncWith { body, .. }
        | StmtKind::With { body, .. } => {
            for stmt in body {
                parse_stmt(stmt, project_root, file_path, third_party_packages);
            }
        }
        StmtKind::For { body, orelse, .. }
        | StmtKind::AsyncFor { body, orelse, .. }
        | StmtKind::While { body, orelse, .. }
        | StmtKind::If { body, orelse, .. } => {
            for stmt in body {
                parse_stmt(stmt, project_root, file_path, third_party_packages);
            }

            for stmt in orelse {
                parse_stmt(stmt, project_root, file_path, third_party_packages);
            }
        }
        StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
        } => {
            for stmt in body {
                parse_stmt(stmt, project_root, file_path, third_party_packages);
            }

            for excepthandler in handlers {
                let ExcepthandlerKind::ExceptHandler { body, .. } = &excepthandler.node;
                for stmt in body {
                    parse_stmt(stmt, project_root, file_path, third_party_packages);
                }
            }

            for stmt in orelse {
                parse_stmt(stmt, project_root, file_path, third_party_packages);
            }

            for stmt in finalbody {
                parse_stmt(stmt, project_root, file_path, third_party_packages);
            }
        }
        _ => (),
    }
}

/// Check if the module is locally imported
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

/// Check if the module is a third-party import or not
/// return module base if it is otherwise return None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_third_party_packages() {
        let content = "import requests
from uuid import UUID
from . import another
from .new import local
from ..again import loc
from django.http import (
    Http404,
    JsonResponse,
)

from .another import one, two, three
import os, sys

def f():
    import pandas
    from .new import local
    print('test')
";
        let file_path = PathBuf::from("./t.py");
        let root = PathBuf::from(".");
        let python_ast = parser::parse_program(&content, &file_path.to_string_lossy()).unwrap();
        assert_eq!(
            find_third_party_packages(&root, &file_path, &python_ast),
            HashSet::from([
                "requests".to_string(),
                "django".to_string(),
                "pandas".to_string()
            ])
        );
    }

    #[test]
    fn test_run() {
        let root = PathBuf::from("./examples");
        assert_eq!(
            run(root),
            (
                5,
                HashSet::from([
                    "pandas".to_string(),
                    "else_package".to_string(),
                    "if_package".to_string(),
                    "except_package".to_string(),
                    "c_package".to_string(),
                    "for_package".to_string(),
                    "f_package".to_string(),
                    "nested_if_except_package".to_string(),
                    "m_package".to_string(),
                    "celery".to_string(),
                    "requests".to_string(),
                    "try_else_package".to_string(),
                    "m_if_package".to_string(),
                    "nested_m_if_package".to_string(),
                    "try_finally_package".to_string(),
                    "django".to_string(),
                    "elif2_package".to_string(),
                    "with_package".to_string(),
                    "elif_package".to_string(),
                    "for_else_package".to_string(),
                    "while_package".to_string(),
                    "try_package".to_string()
                ])
            )
        );
    }
}
