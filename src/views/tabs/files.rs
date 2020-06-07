#![allow(dead_code)]

use std::collections::HashMap;
use deluge_rpc::{FilePriority, Query};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column { Filename, Size, Progress, Priority }
impl AsRef<str> for Column {
    fn as_ref(&self) -> &'static str {
        match self {
            Self::Filename => "Filename",
            Self::Size => "Size",
            Self::Progress => "Progress",
            Self::Priority => "Priority",
        }
    }
}

struct File {
    index: usize,
    path: String,
    size: u64,
    progress: f64,
    priority: FilePriority,
}

#[derive(Default)]
struct Directory {
    children: HashMap<String, DirEntry>,
    num_leaves: usize,
    progress: f64,
}

enum DirEntry {
    File(File),
    Directory(Directory),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct QueryFile {
    index: usize,
    offset: u64,
    path: String,
    size: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Query)]
struct FilesQuery {
    files: Vec<QueryFile>,
    file_progress: Vec<f64>,
    file_priorities: Vec<FilePriority>,
}

fn build_file_tree(query: FilesQuery) -> Directory {
    let FilesQuery { files, file_progress, file_priorities } = query;

    assert_eq!(files.len(), file_progress.len());
    assert_eq!(files.len(), file_priorities.len());

    let mut root = Directory::default();
 
    for file in files.into_iter() {
        let mut cwd = &mut root;
        
        let mut iter = file.path.split('/').peekable();

        while let Some(segment) = iter.next() {
            let segment = String::from(segment);

            cwd.progress *= cwd.num_leaves as f64;
            cwd.num_leaves += 1;
            cwd.progress += file_progress[file.index];
            cwd.progress /= cwd.num_leaves as f64;

            if iter.peek().is_none() {
                assert!(!cwd.children.contains_key(&segment), "unexpected dir entry");
                let f = File {
                    index: file.index,
                    size: file.size,
                    path: file.path,
                    progress: file_progress[file.index],
                    priority: file_priorities[file.index],
                };
                cwd.children.insert(segment, DirEntry::File(f));
                break;
            } else {
                let entry = cwd.children
                    .entry(segment)
                    .or_insert(DirEntry::Directory(Directory::default()));

                cwd = match entry {
                    DirEntry::Directory(ref mut dir) => dir,
                    DirEntry::File(_) => panic!("unexpected file"),
                }
            }
        }
    }

    root
}

